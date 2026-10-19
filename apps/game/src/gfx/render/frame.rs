//! Drawing a frame: the passes, the terrain, the sun camera and the shared uniforms.

use super::*;
use crate::gfx::models::ModelPass;

impl Renderer {
    pub fn render(&mut self, frame: &Frame) {
        let Some((surface, config)) = &self.surface else {
            // Headless and profiling (native): draw into a scratch target, so the frame's GPU
            // work is done and timed (`draw.gpu`) as in the windowed game.
            #[cfg(not(target_arch = "wasm32"))]
            if crate::platform::perf::enabled() {
                let target = self.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("profile target"),
                    size: wgpu::Extent3d {
                        width: self.size.0,
                        height: self.size.1,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: self.format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                });
                self.draw(&target.create_view(&Default::default()), frame);
            }
            return;
        };
        let texture = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            other => {
                log::warn!("surface unavailable: {other:?}");
                if matches!(
                    other,
                    wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost
                ) {
                    surface.configure(&self.device, config);
                }
                return;
            }
        };
        let view = texture.texture.create_view(&Default::default());
        self.draw(&view, frame);
        self.queue.present(texture);
    }

    pub(super) fn draw_terrain<'p>(
        &'p self,
        pass: &mut wgpu::RenderPass<'p>,
        globals: &'p wgpu::BindGroup,
        which: Option<ModelPass>,
    ) {
        self.draw_terrain_with(pass, globals, &self.terrain_pipeline, which);
    }

    /// Decide which terrain tiles `which` draws: those whose box reaches its view
    /// (`view_proj`). In the reflection, a tile wholly below the water (`below`) is skipped
    /// too: the terrain shader discards everything below the water there (`reflect_clip`).
    pub(super) fn cull_terrain(&mut self, which: ModelPass, view_proj: &Mat4, below: Option<f32>) {
        let Some(t) = &mut self.terrain else { return };
        let frustum = crate::gfx::models::Frustum::new(view_proj);
        t.visible[which as usize] = t
            .bounds
            .iter()
            .map(|&(lo, hi)| {
                below.is_none_or(|level| hi.z >= level - 0.001) && frustum.sees_box(lo, hi)
            })
            .collect();
    }

    pub(super) fn draw_terrain_with<'p>(
        &'p self,
        pass: &mut wgpu::RenderPass<'p>,
        globals: &'p wgpu::BindGroup,
        pipeline: &'p wgpu::RenderPipeline,
        which: Option<ModelPass>,
    ) {
        let Some(t) = &self.terrain else { return };
        // Without a pass (the minimap), every tile.
        let visible = which.map_or(&[][..], |w| &t.visible[w as usize]);
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, globals, &[]);
        pass.set_bind_group(1, &t.bind_group, &[]);
        pass.set_index_buffer(t.indices.slice(..), wgpu::IndexFormat::Uint16);
        // One draw per 32×32-cell tile, each with its own 33×33 vertices (as the game does).
        // Binding at an offset avoids base-vertex, which WebGL2 lacks.
        let tile_bytes = (TILE_VERTS * TILE_VERTS * 16) as u64;
        for tile in 0..t.tiles as u64 {
            if visible.get(tile as usize) == Some(&false) {
                continue;
            }
            pass.set_vertex_buffer(
                0,
                t.vertices.slice(tile * tile_bytes..(tile + 1) * tile_bytes),
            );
            pass.draw_indexed(0..t.index_count, 0, 0..1);
        }
    }

    pub(super) fn scene_pass<'e>(
        encoder: &'e mut wgpu::CommandEncoder,
        label: &str,
        color: &'e wgpu::TextureView,
        depth: &'e wgpu::TextureView,
        clear: Option<wgpu::Color>,
    ) -> wgpu::RenderPass<'e> {
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(label),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: clear.map_or(wgpu::LoadOp::Load, wgpu::LoadOp::Clear),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: Some(wgpu::Operations {
                    load: if clear.is_some() {
                        wgpu::LoadOp::Clear(1.0)
                    } else {
                        wgpu::LoadOp::Load
                    },
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        })
    }

    pub(super) fn draw(&mut self, view: &wgpu::TextureView, frame: &Frame) {
        let scene = frame.terrain.filter(|_| self.terrain.is_some());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        if let Some(scene) = &scene {
            let water = self.extras.water_level();
            let sun = self.sun_camera(scene);
            self.shadow.lookup = Some(sun.lookup());
            self.write_globals(&self.globals, scene, None);
            if let Some(level) = water {
                self.write_globals(&self.reflect_globals, scene, Some(level));
            }
            self.last_light = Some(scene.lighting);
            self.extras.place_sky(&self.queue, scene.eye, scene.far);
            let fog = scene.lighting.fog_color;
            let clear = wgpu::Color {
                r: fog.x as f64,
                g: fog.y as f64,
                b: fog.z as f64,
                a: 0.0,
            };
            // `SunLight::DrawShadowMap`: every caster's depth pass (terrain tiles included; the
            // sky's has no programs). It does nothing at shadow detail 0.
            if self.video.shadow_detail > 0 {
                // `SunLight::AddDrawItems` (0x1801097e0) begins the sun's draw group with the
                // sun as its cull camera but group 0's camera as its camera, so the detail
                // levels are measured from the scene's eye (*inferred*: group 0 is the scene's).
                self.models.cull(
                    &self.device,
                    &self.queue,
                    ModelPass::Shadow,
                    &sun.depth_view_proj(self.shadow.size),
                    scene.eye,
                );
                self.cull_terrain(
                    ModelPass::Shadow,
                    &sun.depth_view_proj(self.shadow.size),
                    None,
                );
                let mut pass = self.shadow.begin_pass(&mut encoder);
                self.draw_terrain_with(
                    &mut pass,
                    &self.shadow_bind,
                    &self.terrain_depth_pipeline,
                    Some(ModelPass::Shadow),
                );
                self.models.draw_depth(&mut pass, &self.shadow_bind);
            }
            if let (Some(level), true) = (water, self.video.reflection >= 2) {
                // `ReflectionImageBuffer::Render` (0x1800a7ad0) mirrors the camera's
                // camera-to-world in the water plane, so its position is the mirrored eye.
                self.models.cull(
                    &self.device,
                    &self.queue,
                    ModelPass::Reflection,
                    &reflected(&scene.view_proj, level),
                    Vec3::new(scene.eye.x, scene.eye.y, 2.0 * level - scene.eye.z),
                );
            }
            if let (Some(level), true) = (water, self.video.reflection >= 1) {
                self.cull_terrain(
                    ModelPass::Reflection,
                    &reflected(&scene.view_proj, level),
                    Some(level),
                );
            }
            self.models.cull(
                &self.device,
                &self.queue,
                ModelPass::Scene,
                &scene.view_proj,
                scene.eye,
            );
            self.cull_terrain(ModelPass::Scene, &scene.view_proj, None);
            let t = &self.targets;
            if water.is_some() {
                let mut pass = Self::scene_pass(
                    &mut encoder,
                    "reflection",
                    t.reflection_target(),
                    &t.reflection_depth,
                    Some(clear),
                );
                // `ReflectionImageBuffer::Render` draws its group at the configured
                // ReflectionDetail: None, Terrain or Everything.
                self.extras.draw_sky(&mut pass, &self.reflect_bind);
                if self.video.reflection >= 1 {
                    self.draw_terrain(&mut pass, &self.reflect_bind, Some(ModelPass::Reflection));
                }
                if self.video.reflection >= 2 {
                    self.models
                        .draw(&mut pass, &self.reflect_bind, ModelPass::Reflection);
                }
            }
            if water.is_some() {
                t.resolve_reflection(&mut encoder);
            }
            {
                let mut pass = Self::scene_pass(
                    &mut encoder,
                    "scene",
                    t.scene_target(),
                    &t.depth,
                    Some(clear),
                );
                self.extras.draw_sky(&mut pass, &self.globals_bind);
                self.draw_terrain(&mut pass, &self.globals_bind, Some(ModelPass::Scene));
                self.models
                    .draw(&mut pass, &self.globals_bind, ModelPass::Scene);
                self.extras.draw_decals(&mut pass, &self.globals_bind);
            }
            if water.is_some() {
                t.resolve_scene(&mut encoder);
                encoder.copy_texture_to_texture(
                    t.scene.as_image_copy(),
                    t.frame_copy.as_image_copy(),
                    wgpu::Extent3d {
                        width: self.size.0,
                        height: self.size.1,
                        depth_or_array_layers: 1,
                    },
                );
                let mut pass =
                    Self::scene_pass(&mut encoder, "water", t.scene_target(), &t.depth, None);
                self.extras.draw_water(&mut pass, &self.globals_bind);
            }
            // Particles after the water, as transparent geometry, then the rain and snow.
            self.particles
                .draw(&mut encoder, t.scene_target(), &t.depth, &self.globals_bind);
            let textures = &self.weather_textures;
            self.weather.prepare(
                &self.device,
                &self.queue,
                &self.pending_weather,
                scene.eye,
                &scene.view_proj,
                |m| textures.get(m).cloned().flatten(),
            );
            self.weather
                .draw(&mut encoder, t.scene_target(), &t.depth, &self.globals_bind);
            self.highlight.draw(
                &mut encoder,
                &self.queue,
                t.scene_target(),
                &t.depth,
                scene,
                self.size.0,
            );
            self.cell_overlay.prepare(
                &self.device,
                &self.queue,
                &self.pending_overlay,
                self.overlay_texture.as_ref(),
                scene,
            );
            self.cell_overlay
                .draw(&mut encoder, t.scene_target(), &t.depth);
            self.status_icons.prepare(
                &self.device,
                &self.queue,
                &self.pending_icons,
                &self.icon_sheets,
                scene,
                self.icon_options,
            );
            self.status_icons
                .draw(&mut encoder, t.scene_target(), &t.depth);
            t.resolve_scene(&mut encoder);
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("screen"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(frame.clear),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.targets.screen_depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if scene.is_some() {
                pass.set_pipeline(&self.blit_pipeline);
                pass.set_bind_group(0, &self.targets.blit_bind, &[]);
                pass.draw(0..3, 0..1);
            }
            if let Some((ui, quads)) = frame.ui {
                self.ui
                    .draw(&self.device, &self.queue, &mut pass, ui, quads, self.size);
            }
        }
        self.queue.submit([encoder.finish()]);
        // While profiling (native), wait for the GPU so its time shows as a phase of its own.
        // This changes timing only, never what is drawn.
        #[cfg(not(target_arch = "wasm32"))]
        if crate::platform::perf::enabled() {
            let _p = crate::platform::perf::scope("draw.gpu");
            let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        }
    }

    /// Fit the sun camera to this frame's camera (`SunLight::GetSunCamera`) and write the shadow
    /// pass's globals: the sun's view-projection and axes (billboards face the sun there).
    pub(super) fn sun_camera(&self, scene: &SceneFrame) -> crate::gfx::shadow::SunCamera {
        let camera = crate::gfx::shadow::MainCamera {
            eye: scene.eye,
            right: scene.right,
            forward: scene.forward,
            up: scene.up,
            hfov: scene.hfov,
            vfov: scene.vfov,
            near: scene.near,
        };
        let sun =
            crate::gfx::shadow::sun_camera(&camera, scene.lighting.light_dir, self.shadow.size);
        let frame = SceneFrame {
            view_proj: sun.depth_view_proj(self.shadow.size),
            eye: sun.position,
            far: crate::gfx::shadow::SUN_FAR,
            right: sun.right,
            forward: sun.forward,
            up: sun.up,
            ..*scene
        };
        self.write_globals(&self.shadow_globals, &frame, None);
        sun
    }

    /// Write a pass's globals. With `reflect`, mirror the scene about that water height and clip
    /// what lies below it.
    pub(super) fn write_globals(
        &self,
        buffer: &wgpu::Buffer,
        scene: &SceneFrame,
        reflect: Option<f32>,
    ) {
        let l = &scene.lighting;
        let (view_proj, clip) = match reflect {
            Some(level) => (reflected(&scene.view_proj, level), [1.0, level, 0.0, 0.0]),
            None => (scene.view_proj, [0.0; 4]),
        };
        let globals = Globals {
            view_proj: view_proj.to_cols_array_2d(),
            light_dir: l.light_dir.extend(0.0).to_array(),
            light_color: l.light_color.extend(1.0).to_array(),
            ambient_color: l.ambient_color.extend(1.0).to_array(),
            fog_color: l.fog_color.extend(l.fog_max).to_array(),
            fog: Vec4::new(-l.fog_start, 1.0 / (l.fog_end - l.fog_start), 0.0, 0.0).to_array(),
            user: l.user.to_array(),
            camera_right: scene.right.extend(0.0).to_array(),
            camera_forward: scene.forward.extend(scene.time).to_array(),
            camera_up: scene.up.extend(0.0).to_array(),
            camera_pos: scene.eye.extend(1.0).to_array(),
            clip,
            shadow: self.shadow.lookup.unwrap_or(Mat4::ZERO).to_cols_array_2d(),
            shadow_params: [
                1.0 / self.shadow.size as f32,
                if self.shadow.lookup.is_some() && self.video.shadow_detail > 0 {
                    1.0
                } else {
                    0.0
                },
                self.video.shadow_detail as f32,
                0.0,
            ],
        };
        self.queue
            .write_buffer(buffer, 0, bytemuck::bytes_of(&globals));
    }
}

/// A view-projection mirrored about the water surface at height `level` (the reflection pass).
fn reflected(view_proj: &Mat4, level: f32) -> Mat4 {
    *view_proj
        * Mat4::from_translation(Vec3::new(0.0, 0.0, 2.0 * level))
        * Mat4::from_scale(Vec3::new(1.0, 1.0, -1.0))
}
