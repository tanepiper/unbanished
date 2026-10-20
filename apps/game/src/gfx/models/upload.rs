//! Uploading model groups and their instances to the GPU, and keeping them up to date.

use super::*;

pub(super) struct GpuRange {
    pub(super) kind: MaterialKind,
    pub(super) skinned: bool,
    pub(super) vertices: wgpu::Buffer,
    pub(super) indices: wgpu::Buffer,
    pub(super) index_count: u32,
    pub(super) bind: wgpu::BindGroup,
    pub(super) level: usize,
}

pub(super) struct GpuGroup {
    pub(super) ranges: Vec<GpuRange>,
    pub(super) instances: wgpu::Buffer,
    pub(super) count: u32,
    /// Per-instance bone palettes and their dynamic-offset bind group (skinned groups).
    pub(super) skin: Option<(wgpu::Buffer, wgpu::BindGroup)>,
    pub(super) skinned: bool,
    /// The group's bounds, its instances' transforms and their world spheres (for culling).
    pub(super) bounds: Option<Bounds>,
    pub(super) matrices: Vec<[f32; 16]>,
    pub(super) spheres: Vec<[f32; 4]>,
    /// What each [`ModelPass`] draws this frame.
    pub(super) visible: [Visible; 3],
    /// Each detail level's distance, and the mesh's box its instances' distances are measured
    /// from ([`culling::detail_level`]).
    pub(super) levels: Vec<f32>,
    pub(super) mesh_box: (Vec3, Vec3),
}

impl ModelRenderer {
    pub fn clear(&mut self) {
        self.groups.clear();
        self.views.clear();
    }

    /// Upload every group of a scene (a new world).
    pub fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, models: &SceneModels) {
        self.clear();
        for g in &models.groups {
            let gpu = self.upload_group(device, queue, g);
            self.groups.push(gpu);
        }
    }

    /// One group's ranges, textures (each image uploaded once) and instances.
    pub(super) fn upload_group(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        g: &ModelGroup,
    ) -> GpuGroup {
        let white = self.white.clone();
        let views = &mut self.views;
        let mut view_of = |img: &ModelImage| -> wgpu::TextureView {
            let Some(img) = img else {
                return white.clone();
            };
            // Images are shared between ranges (`ModelImage`); key by the shared allocation.
            let key = std::sync::Arc::as_ptr(img) as usize;
            views
                .entry(key)
                .or_insert_with(|| {
                    let mips: Vec<Vec<u8>> = (0..img.mip_count().max(1))
                        .map_while(|i| {
                            decode_rgba(
                                img.format,
                                (img.width >> i).max(1),
                                (img.height >> i).max(1),
                                img.levels.get(i)?,
                            )
                        })
                        .collect();
                    if mips.is_empty() {
                        return white.clone();
                    }
                    unbanished_gfx::upload_rgba_as(
                        device,
                        queue,
                        "model texture",
                        img.width,
                        img.height,
                        &mips,
                        wgpu::TextureFormat::Rgba8Unorm,
                    )
                })
                .clone()
        };
        let ranges = g
            .ranges
            .iter()
            .map(|r| {
                let mut uniform = [0.0f32; 20];
                uniform[..16].copy_from_slice(&r.rigid);
                uniform[16] = if r.damage.is_some() { 1.0 } else { 0.0 };
                let rigid = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("rigid"),
                    contents: bytemuck::cast_slice(&uniform),
                    usage: wgpu::BufferUsages::UNIFORM,
                });
                let (diffuse, ao, snow, damage) = (
                    view_of(&r.diffuse),
                    view_of(&r.ao),
                    view_of(&r.snow),
                    view_of(&r.damage),
                );
                let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some(&g.name),
                    layout: &self.layout,
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
                            resource: rigid.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 4,
                            resource: wgpu::BindingResource::TextureView(&snow),
                        },
                        wgpu::BindGroupEntry {
                            binding: 5,
                            resource: wgpu::BindingResource::TextureView(&damage),
                        },
                    ],
                });
                GpuRange {
                    kind: r.kind,
                    skinned: r.skinned,
                    vertices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some(&g.name),
                        contents: &r.vertices,
                        usage: wgpu::BufferUsages::VERTEX,
                    }),
                    indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some(&g.name),
                        contents: &r.indices,
                        usage: wgpu::BufferUsages::INDEX,
                    }),
                    index_count: r.index_count,
                    bind,
                    level: r.level,
                }
            })
            .collect();
        let mut gpu = GpuGroup {
            ranges,
            instances: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("instances"),
                contents: bytemuck::cast_slice(&g.instances),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            }),
            count: g.instances.len() as u32,
            skin: self.skin_buffer(device, &g.skins),
            skinned: g.skinned(),
            bounds: Bounds::of(&g.ranges),
            matrices: Vec::new(),
            spheres: Vec::new(),
            visible: Default::default(),
            levels: g.levels.clone(),
            mesh_box: (Vec3::from_array(g.bounds.0), Vec3::from_array(g.bounds.1)),
        };
        gpu.set_matrices(&g.instances);
        gpu
    }

    /// Refresh the instance buffers. Groups are only ever added to a scene, so new ones are
    /// uploaded on their own; re-uploading every group and texture took about 50 ms, a visible
    /// hitch each time a new kind of model appeared (issue #5).
    pub fn update_instances(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        models: &SceneModels,
    ) {
        if self.groups.len() > models.groups.len() {
            self.upload(device, queue, models);
            return;
        }
        for g in &models.groups[self.groups.len()..] {
            let gpu = self.upload_group(device, queue, g);
            self.groups.push(gpu);
        }
        for (gpu, group) in self.groups.iter_mut().zip(&models.groups) {
            let data = bytemuck::cast_slice(&group.instances);
            if data.len() as u64 > gpu.instances.size() {
                gpu.instances = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("grown instances"),
                    contents: data,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                });
            } else if !data.is_empty() {
                queue.write_buffer(&gpu.instances, 0, data);
            }
            gpu.count = group.instances.len() as u32;
            gpu.set_matrices(&group.instances);
            let skins: &[u8] = bytemuck::cast_slice(&group.skins);
            match &gpu.skin {
                Some((buffer, _)) if skins.len() as u64 <= buffer.size() => {
                    if !skins.is_empty() {
                        queue.write_buffer(buffer, 0, skins);
                    }
                }
                _ if !skins.is_empty() => {
                    gpu.skin = skin_buffer(device, &self.skin_layout, &group.skins);
                }
                _ => {}
            }
        }
    }
}

impl GpuGroup {
    /// Take a new set of instance transforms and their world spheres; every pass draws all of
    /// them until it is culled again.
    pub(super) fn set_matrices(&mut self, matrices: &[[f32; 16]]) {
        self.matrices.clear();
        self.matrices.extend_from_slice(matrices);
        self.spheres.clear();
        if let Some(b) = &self.bounds {
            self.spheres.extend(matrices.iter().map(|m| b.sphere(m)));
        }
        self.visible = Default::default();
    }
}
