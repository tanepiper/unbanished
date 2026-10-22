//! Loading meshes and their textures to the GPU, and posing a skinned mesh.

use super::*;

impl Renderer {
    /// Upload a referenced image; also returns its aspect (height / width).
    pub(super) fn texture(
        &self,
        assets: &Assets,
        reference: Option<&str>,
    ) -> Option<(wgpu::TextureView, f32)> {
        let reference = reference?;
        let img = match assets.image(reference) {
            Ok(img) => img,
            Err(e) => {
                log::warn!("{reference}: {e}");
                return None;
            }
        };
        let aspect = img.height as f32 / img.width.max(1) as f32;
        upload_image(&self.device, &self.queue, reference, &img).map(|v| (v, aspect))
    }

    pub fn load_mesh(&self, assets: &Assets, name: &str) -> unbanished_formats::Result<GpuMesh> {
        let mesh = assets.mesh(name)?;
        let mut parts = Vec::new();
        let world = unbanished_gfx::model::bind_world(&mesh);
        let set = unbanished_gfx::model::material_set(&mesh, 0);
        // Parts are detail levels; the asset browser displays the most detailed one.
        for part in mesh.parts.iter().take(1) {
            for range in &part.ranges {
                let (index_data, vertex_data, _) = part.geometry(range);
                if range.index_count == 0 || index_data.is_empty() {
                    continue;
                }
                // Shared with the game: material instance, shader kind, textures, rigid node.
                let m = unbanished_gfx::model::range_material(&world, set, range, &mut |r| {
                    assets
                        .material_instance(r)
                        .map_err(|e| log::warn!("{r}: {e}"))
                        .ok()
                });
                let (diffuse, _) = self
                    .texture(assets, m.diffuse.as_deref())
                    .unwrap_or_else(|| (self.white.clone(), 1.0));
                let ao = self
                    .texture(assets, part.ao_texture.as_deref())
                    .map_or_else(|| self.white.clone(), |t| t.0);
                // Character materials select one outfit of the atlas (the first here), with the
                // same rule the game uses (`unbanished_gfx::material`).
                let uv_transform: [f32; 4] = if m.kind == MaterialKind::Character {
                    character_uv_transform(0)
                } else {
                    [1.0, 1.0, 0.0, 0.0]
                };
                // Rigid triangle lists use the node's world transform; weighted lists already
                // apply world * inverse_bind through the skin palette in the vertex shader.
                let rigid = m.rigid;
                let mut material_params = uv_transform.to_vec();
                material_params.extend_from_slice(&rigid.to_matrix());
                let params = self
                    .device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("material params"),
                        contents: bytemuck::cast_slice(&material_params),
                        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    });
                let textures = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some(&part.name),
                    layout: &self.texture_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&diffuse),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(&ao),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Sampler(&self.sampler),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: params.as_entire_binding(),
                        },
                    ],
                });
                let buffer = |data: &[u8], usage| {
                    self.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some(&part.name),
                            contents: data,
                            usage,
                        })
                };
                // Index buffers must be 4-byte aligned in size for writes on some backends.
                let mut indices = index_data.to_vec();
                indices.resize(indices.len().next_multiple_of(4), 0);
                let first = range.start_index as usize * 2;
                let end = (range.start_index + range.index_count) as usize * 2;
                let wire_data: Vec<u16> = index_data[first..end]
                    .chunks_exact(6)
                    .flat_map(|tri| {
                        let a = u16::from_le_bytes([tri[0], tri[1]]);
                        let b = u16::from_le_bytes([tri[2], tri[3]]);
                        let c = u16::from_le_bytes([tri[4], tri[5]]);
                        [a, b, b, c, c, a]
                    })
                    .collect();
                let wire_index_count = wire_data.len() as u32;
                parts.push(GpuPart {
                    vertices: buffer(vertex_data, wgpu::BufferUsages::VERTEX),
                    indices: buffer(&indices, wgpu::BufferUsages::INDEX),
                    wire_indices: self.device.create_buffer_init(
                        &wgpu::util::BufferInitDescriptor {
                            label: Some("wireframe indices"),
                            contents: bytemuck::cast_slice(&wire_data),
                            usage: wgpu::BufferUsages::INDEX,
                        },
                    ),
                    wire_index_count,
                    index_range: range.start_index as u32
                        ..(range.start_index + range.index_count) as u32,
                    base_vertex: range.start_vertex,
                    skinned: m.skinned,
                    rigid_bone: m.rigid_bone,
                    textures,
                    material_params: params,
                });
            }
        }
        let matrices = unbanished_gfx::skin::palette_matrices(&bind_palette(&mesh.bones));
        let skin_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("skin"),
                contents: bytemuck::cast_slice(&matrices),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
        let skin = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("skin"),
            layout: &self.skin_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: skin_buffer.as_entire_binding(),
            }],
        });
        let (min, max) = (Vec3::from(mesh.bounds_min), Vec3::from(mesh.bounds_max));
        Ok(GpuMesh {
            parts,
            skin,
            skin_buffer,
            bones: mesh.bones,
            center: (min + max) * 0.5,
            radius: ((max - min).length() * 0.5).max(0.1),
        })
    }

    pub fn set_animation_pose(&self, mesh: &GpuMesh, clip: &AnimationClip, seconds: f32) {
        if clip.frames.is_empty()
            || clip.bone_count != mesh.bones.len()
            || mesh.bones.len() > MAX_BONES
            || clip.sample_rate <= 0.0
        {
            return;
        }
        // The original's Animation::GetPoseAtTime, shared with the game (`unbanished_game`).
        let locals = unbanished_game::behaviour::animation::sample_pose(
            clip,
            unbanished_game::scheduler::ticks(seconds.max(0.0)),
            0,
        );
        if locals.len() != mesh.bones.len() {
            return;
        }
        let world = world_transforms(&mesh.bones, &locals);
        for part in &mesh.parts {
            if let Some(bone) = part.rigid_bone.and_then(|i| world.get(i)) {
                self.queue.write_buffer(
                    &part.material_params,
                    16,
                    bytemuck::cast_slice(&bone.to_matrix()),
                );
            }
        }
        let matrices = unbanished_gfx::skin::palette_matrices(
            &unbanished_formats::skeleton::palette(&mesh.bones, &locals),
        );
        self.queue
            .write_buffer(&mesh.skin_buffer, 0, bytemuck::cast_slice(&matrices));
    }
}
