//! The minimap: updating and drawing it.

use super::*;

impl Renderer {
    /// MapUI's images. `FUN_1400aa610` renders the terrain and water geometries once, on the
    /// first game draw: an orthographic camera over the map (±size/2, near 0, far 512) at
    /// height 100 looking straight down (pitch -90), fog pushed to 1000–1001, into a size×size
    /// image. `Context::SetUserPosition((0, 0, 1, 0))` sets `pc_userPosition`: no snow, no
    /// autumn colour, full leaf loss, no rain. Its scene has no light (only `SunLight::Draw`
    /// and `SpotLight::Draw` call `Context::SetLight`; `AmbientLight::Draw` is empty), and it
    /// runs before the frame's `Renderer::Draw`, so the light is the last scene's sun:
    /// `Context::Reset` clears the light block but not the shader constants it was copied to
    /// (`+0x268`). The overlay
    /// (`FUN_1400aa5b0`) is uploaded when it changes.
    pub fn update_minimap(&mut self, view: &crate::view::scene::SceneView) {
        let Some((revision, size, rgba)) = &view.minimap_overlay else {
            return;
        };
        if self.minimap.is_none() && self.terrain.is_some() {
            self.render_minimap(*size, &view.lighting);
        }
        if self.minimap_overlay != *revision {
            let texture = unbanished_gfx::upload_rgba_as(
                &self.device,
                &self.queue,
                "minimap overlay",
                *size,
                *size,
                std::slice::from_ref(rgba),
                wgpu::TextureFormat::Rgba8Unorm,
            );
            self.ui
                .set_external(&self.device, crate::loops::MINIMAP_OVERLAY, &texture);
            self.minimap_overlay = *revision;
        }
    }

    pub(super) fn render_minimap(&mut self, n: u32, lighting: &Lighting) {
        let targets = Targets::new(
            &self.device,
            (n, n),
            (n, n),
            &self.blit_layout,
            &self.blit_sampler,
        );
        let half = n as f32 * 0.5;
        let eye = Vec3::new(0.0, 0.0, 100.0);
        use glam::camera::rh::{proj::directx::orthographic, view::look_at_mat4};
        let view_proj = orthographic(-half, half, -half, half, 0.0, 512.0)
            * look_at_mat4(eye, Vec3::ZERO, Vec3::Y);
        // The minimap's scene has no light: the sun, its colour and the ambient are those the
        // last scene drawn set (the menu's, or the previous game's).
        let mut lighting = *lighting;
        if let Some(last) = self.last_light {
            lighting.light_dir = last.light_dir;
            lighting.light_color = last.light_color;
            lighting.ambient_color = last.ambient_color;
        }
        lighting.user = Vec4::new(0.0, 0.0, 1.0, 0.0);
        lighting.fog_start = 1000.0;
        lighting.fog_end = 1001.0;
        lighting.fog_color = Vec3::ONE;
        let scene = SceneFrame {
            view_proj,
            lighting,
            eye,
            far: 512.0,
            right: Vec3::X,
            forward: Vec3::NEG_Z,
            up: Vec3::Y,
            time: 0.0,
            hfov: 0.0,
            vfov: 0.0,
            near: 0.0,
        };
        // The minimap scene has no sun light, so no shadow map.
        self.shadow.lookup = None;
        self.extras.bind_targets(
            &self.device,
            &targets.frame_copy_view,
            &targets.reflection_view,
        );
        let water = self.extras.water_level();
        self.write_globals(&self.globals, &scene, None);
        if let Some(level) = water {
            self.write_globals(&self.reflect_globals, &scene, Some(level));
        }
        let output = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("minimap"),
            size: wgpu::Extent3d {
                width: n,
                height: n,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let output_view = output.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        // `Context::Clear(7, 0xffff00ff)`.
        let clear = wgpu::Color {
            r: 1.0,
            g: 0.0,
            b: 1.0,
            a: 1.0,
        };
        // The minimap's renderer is a fresh `Graphics::Renderer` (`FUN_1400aa610`) with no
        // reflection pass, so the water reflects nothing: an opaque black reflection (inferred
        // from the original's dark lakes and blue-grey rivers on the minimap).
        if water.is_some() {
            let _pass = Self::scene_pass(
                &mut encoder,
                "minimap reflection",
                targets.reflection_target(),
                &targets.reflection_depth,
                Some(wgpu::Color::BLACK),
            );
        }
        if water.is_some() {
            targets.resolve_reflection(&mut encoder);
        }
        {
            let mut pass = Self::scene_pass(
                &mut encoder,
                "minimap",
                targets.scene_target(),
                &targets.depth,
                Some(clear),
            );
            self.draw_terrain(&mut pass, &self.globals_bind, None);
        }
        targets.resolve_scene(&mut encoder);
        if water.is_some() {
            encoder.copy_texture_to_texture(
                targets.scene.as_image_copy(),
                targets.frame_copy.as_image_copy(),
                wgpu::Extent3d {
                    width: n,
                    height: n,
                    depth_or_array_layers: 1,
                },
            );
            {
                let mut pass = Self::scene_pass(
                    &mut encoder,
                    "minimap water",
                    targets.scene_target(),
                    &targets.depth,
                    None,
                );
                self.extras.draw_water(&mut pass, &self.globals_bind);
            }
            targets.resolve_scene(&mut encoder);
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("minimap blit"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &output_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &targets.screen_depth,
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
            pass.set_pipeline(&self.blit_pipeline);
            pass.set_bind_group(0, &targets.blit_bind, &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit([encoder.finish()]);
        self.extras.bind_targets(
            &self.device,
            &self.targets.frame_copy_view,
            &self.targets.reflection_view,
        );
        self.ui
            .set_external(&self.device, crate::loops::MINIMAP, &output_view);
        self.minimap = Some(output);
    }
}
