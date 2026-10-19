//! Uploading data to the GPU: UI sheets, images, the scene and the terrain.

use super::*;

impl Renderer {
    /// The UI's resources were made again (mods reloaded): its sheets upload afresh.
    pub fn reset_ui_sheets(&mut self) {
        self.ui.clear_sheets();
    }

    /// Seconds since the current loop started (animated UI materials).
    pub fn set_ui_time(&mut self, seconds: f32) {
        self.ui.set_time(seconds);
    }

    pub(super) fn upload(
        &self,
        label: &str,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        mips: &[Vec<u8>],
    ) -> wgpu::TextureView {
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: mips.len() as u32,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for (level, data) in mips.iter().enumerate() {
            let (w, h) = ((width >> level).max(1), (height >> level).max(1));
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: level as u32,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(w * 4),
                    rows_per_image: Some(h),
                },
                wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
            );
        }
        texture.create_view(&Default::default())
    }

    pub(super) fn upload_image(&self, label: &str, img: &ImageBuffer) -> wgpu::TextureView {
        let mips: Vec<Vec<u8>> = (0..img.mip_count().max(1))
            .map(|i| {
                decode_rgba(
                    img.format,
                    (img.width >> i).max(1),
                    (img.height >> i).max(1),
                    &img.levels[i],
                )
                .unwrap_or_else(|| {
                    vec![255; (((img.width >> i).max(1)) * ((img.height >> i).max(1)) * 4) as usize]
                })
            })
            .collect();
        self.upload(
            label,
            img.width,
            img.height,
            wgpu::TextureFormat::Rgba8Unorm,
            &mips,
        )
    }

    /// Upload a scene's terrain, models, sky, water and decals if it is not the one on the GPU.
    pub fn set_scene(&mut self, view: &crate::view::scene::SceneView) {
        let _p = crate::platform::perf::scope("draw.set_scene");
        self.highlight.upload(&self.device, &view.highlight);
        self.pending_icons.clone_from(&view.status_icons);
        self.particles
            .prepare(&self.device, &self.queue, &view.particles, |m| {
                view.particle_textures.get(m).cloned().unwrap_or_default()
            });
        self.pending_weather.clone_from(&view.weather);
        for (k, v) in &view.weather_textures {
            if !self.weather_textures.contains_key(k) {
                self.weather_textures.insert(k.clone(), v.clone());
            }
        }
        self.pending_overlay.clone_from(&view.cell_overlay);
        if self.overlay_texture.is_none() {
            self.overlay_texture.clone_from(&view.overlay_texture);
        }
        for (k, v) in &view.icon_sheets {
            if !self.icon_sheets.contains_key(k) {
                self.icon_sheets.insert(k.clone(), v.clone());
            }
        }
        if self.world_id == view.id && self.terrain_revision != view.terrain.revision {
            if let Some(t) = &self.terrain {
                self.queue
                    .write_buffer(&t.vertices, 0, &view.terrain.mesh.vertex_bytes());
            }
        }
        self.terrain_revision = view.terrain.revision;
        if self.world_id != view.id {
            self.set_terrain(&view.terrain);
            match &view.models {
                Some(m) => self.models.upload(&self.device, &self.queue, m),
                None => self.models.clear(),
            }
            match &view.extras {
                Some(e) => self.extras.upload(&self.device, &self.queue, e),
                None => self.extras.clear(),
            }
            self.extras.bind_targets(
                &self.device,
                &self.targets.frame_copy_view,
                &self.targets.reflection_view,
            );
            self.world_id = view.id;
            self.decals_revision = view.decals_revision;
            self.models_revision = view.models.as_ref().map_or(0, |m| m.revision);
            self.minimap = None;
            self.minimap_overlay = 0;
        } else if self.decals_revision != view.decals_revision {
            if let Some(e) = &view.extras {
                let _p = crate::platform::perf::scope("draw.upload_decals");
                self.extras
                    .upload_decals(&self.device, &self.queue, &e.decals);
            }
            self.decals_revision = view.decals_revision;
        }
        if self.world_id != view.id {
        } else if let Some(models) = &view.models {
            if self.models_revision != models.revision {
                let _p = crate::platform::perf::scope("draw.update_instances");
                self.models
                    .update_instances(&self.device, &self.queue, models);
                self.models_revision = models.revision;
            }
        }
    }

    /// Upload a terrain's mesh, surface image and textures.
    pub fn set_terrain(&mut self, world: &Terrain) {
        let surface = world.surface.clone();
        let size = world.size as u32;
        let views = [
            self.upload_image("grass", &world.textures.grass),
            self.upload_image("dirt", &world.textures.dirt),
            self.upload_image("snow", &world.textures.snow),
            self.upload_image("rock", &world.textures.rock),
            self.upload(
                "transitionMap",
                size,
                size,
                wgpu::TextureFormat::Bgra8Unorm,
                &[surface],
            ),
        ];
        let sampler = |mode| {
            self.device.create_sampler(&wgpu::SamplerDescriptor {
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                ..crate::gfx::scene_gfx::material_sampler(mode)
            })
        };
        let wrap = sampler(wgpu::AddressMode::Repeat);
        let clamp = sampler(wgpu::AddressMode::ClampToEdge);
        let mut entries: Vec<wgpu::BindGroupEntry> = views
            .iter()
            .enumerate()
            .map(|(i, v)| wgpu::BindGroupEntry {
                binding: i as u32,
                resource: wgpu::BindingResource::TextureView(v),
            })
            .collect();
        entries.push(wgpu::BindGroupEntry {
            binding: 5,
            resource: wgpu::BindingResource::Sampler(&wrap),
        });
        entries.push(wgpu::BindGroupEntry {
            binding: 6,
            resource: wgpu::BindingResource::Sampler(&clamp),
        });
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("terrain material"),
            layout: &self.material_layout,
            entries: &entries,
        });
        let mut indices: Vec<u8> = world
            .mesh
            .indices
            .iter()
            .flat_map(|i| i.to_le_bytes())
            .collect();
        // Index buffers must be a multiple of 4 bytes.
        indices.resize(indices.len().next_multiple_of(4), 0);
        self.terrain = Some(TerrainGpu {
            vertices: self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("terrain vertices"),
                    contents: &world.mesh.vertex_bytes(),
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                }),
            indices: self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("terrain indices"),
                    contents: &indices,
                    usage: wgpu::BufferUsages::INDEX,
                }),
            index_count: world.mesh.indices.len() as u32,
            tiles: (world.mesh.tiles_per_side * world.mesh.tiles_per_side) as u32,
            bind_group,
        });
    }
}
