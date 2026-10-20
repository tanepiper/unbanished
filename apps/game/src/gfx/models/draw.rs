//! Drawing the models: culling each pass's instances, then the opaque, alpha, foliage and depth
//! draws.

use super::*;

impl ModelRenderer {
    /// Decide what `pass` draws this frame: the instances whose spheres reach its view
    /// (`view_proj`), in their order, and for a mesh with several detail levels the level of
    /// each, measured from the pass's camera position `eye`. Call it before each pass's draw.
    pub fn cull(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pass: ModelPass,
        view_proj: &Mat4,
        eye: Vec3,
    ) {
        let frustum = Frustum::new(view_proj);
        let p = pass as usize;
        self.packed.clear();
        for g in &mut self.groups {
            if g.levels.len() > 1 {
                // `MeshInstance::AddDrawItems` (0x1801037b0) picks the level per instance and
                // draw group; each level here gets its own list, in the instances' order.
                let culled = g.bounds.is_some() && g.spheres.len() == g.count as usize;
                let (lo, hi) = g.mesh_box;
                let chosen: Vec<Option<usize>> = g
                    .matrices
                    .iter()
                    .enumerate()
                    .map(|(i, m)| {
                        (!culled || frustum.sees(&g.spheres[i]))
                            .then(|| detail_level(&g.levels, lo, hi, m, eye))
                    })
                    .collect();
                let lists = (0..g.levels.len())
                    .map(|level| {
                        let at = |i: &usize| chosen[*i] == Some(level);
                        if g.skinned {
                            return Visible::Indices(
                                (0..g.count).filter(|&i| at(&(i as usize))).collect(),
                            );
                        }
                        let offset = (self.packed.len() * 64) as u64;
                        let before = self.packed.len();
                        self.packed.extend(
                            (0..g.matrices.len())
                                .filter(|i| at(i))
                                .map(|i| g.matrices[i]),
                        );
                        Visible::Packed {
                            offset,
                            count: (self.packed.len() - before) as u32,
                        }
                    })
                    .collect();
                g.visible[p] = Visible::Levels(lists);
                continue;
            }
            if g.bounds.is_none() || g.spheres.len() != g.count as usize {
                g.visible[p] = Visible::All;
                continue;
            }
            let seen = g.spheres.iter().filter(|s| frustum.sees(s)).count();
            g.visible[p] = if seen == g.spheres.len() {
                Visible::All
            } else if g.skinned {
                Visible::Indices(
                    (0..g.count)
                        .filter(|&i| frustum.sees(&g.spheres[i as usize]))
                        .collect(),
                )
            } else {
                let offset = (self.packed.len() * 64) as u64;
                self.packed.extend(
                    g.matrices
                        .iter()
                        .zip(&g.spheres)
                        .filter(|(_, s)| frustum.sees(s))
                        .map(|(m, _)| *m),
                );
                Visible::Packed {
                    offset,
                    count: seen as u32,
                }
            };
        }
        if self.packed.is_empty() {
            return;
        }
        let data: &[u8] = bytemuck::cast_slice(&self.packed);
        let buffer = &mut self.pass_buffers[p];
        if buffer.as_ref().is_none_or(|b| b.size() < data.len() as u64) {
            *buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("visible instances"),
                size: (data.len() as u64).next_power_of_two(),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }
        queue.write_buffer(buffer.as_ref().unwrap(), 0, data);
    }

    pub fn draw<'p>(
        &'p self,
        pass: &mut wgpu::RenderPass<'p>,
        globals: &'p wgpu::BindGroup,
        which: ModelPass,
    ) {
        self.draw_with(
            pass,
            globals,
            &self.pipelines,
            &self.skinned_pipelines,
            which,
        );
    }

    /// The models' depth into the sun's shadow map.
    pub fn draw_depth<'p>(&'p self, pass: &mut wgpu::RenderPass<'p>, globals: &'p wgpu::BindGroup) {
        self.draw_with(
            pass,
            globals,
            &self.depth_pipelines,
            &self.skinned_depth_pipelines,
            ModelPass::Shadow,
        );
    }

    pub(super) fn draw_with<'p>(
        &'p self,
        pass: &mut wgpu::RenderPass<'p>,
        globals: &'p wgpu::BindGroup,
        pipelines: &'p [wgpu::RenderPipeline; 6],
        skinned_pipelines: &'p [wgpu::RenderPipeline; 2],
        which: ModelPass,
    ) {
        if self.groups.is_empty() {
            return;
        }
        let p = which as usize;
        pass.set_bind_group(0, globals, &[]);
        for (i, kind) in KINDS.into_iter().enumerate() {
            pass.set_pipeline(&pipelines[i]);
            for g in &self.groups {
                // Groups emptied by removal or a moved placement ghost keep their assets but
                // have nothing to draw (and an empty instance buffer cannot be bound).
                if g.count == 0 || g.instances.size() == 0 {
                    continue;
                }
                for level in 0..g.levels.len().max(1) {
                    let Some(visible) = g.visible[p].level(level) else {
                        continue;
                    };
                    let count = match visible {
                        Visible::Packed { offset, count } => {
                            let (Some(buffer), true) = (&self.pass_buffers[p], *count > 0) else {
                                continue;
                            };
                            pass.set_vertex_buffer(
                                1,
                                buffer.slice(*offset..*offset + *count as u64 * 64),
                            );
                            *count
                        }
                        _ => {
                            pass.set_vertex_buffer(1, g.instances.slice(..));
                            g.count
                        }
                    };
                    for r in g
                        .ranges
                        .iter()
                        .filter(|r| r.kind == kind && !r.skinned && r.level == level)
                    {
                        pass.set_bind_group(1, &r.bind, &[]);
                        pass.set_vertex_buffer(0, r.vertices.slice(..));
                        pass.set_index_buffer(r.indices.slice(..), wgpu::IndexFormat::Uint16);
                        pass.draw_indexed(0..r.index_count, 0, 0..count);
                    }
                }
            }
        }
        // Skinned ranges: one draw per instance with its palette (dynamic offset).
        for (pipeline, character) in [
            (&skinned_pipelines[0], false),
            (&skinned_pipelines[1], true),
        ] {
            pass.set_pipeline(pipeline);
            for g in &self.groups {
                let Some((_, skin)) = &g.skin else { continue };
                if g.count == 0 || g.instances.size() == 0 {
                    continue;
                }
                pass.set_vertex_buffer(1, g.instances.slice(..));
                for r in g
                    .ranges
                    .iter()
                    .filter(|r| r.skinned && (r.kind == MaterialKind::Character) == character)
                {
                    let Some(visible) = g.visible[p].level(r.level) else {
                        continue;
                    };
                    pass.set_bind_group(1, &r.bind, &[]);
                    pass.set_vertex_buffer(0, r.vertices.slice(..));
                    pass.set_index_buffer(r.indices.slice(..), wgpu::IndexFormat::Uint16);
                    let all: Vec<u32>;
                    let shown = match visible {
                        Visible::Indices(list) => list.as_slice(),
                        _ => {
                            all = (0..g.count).collect();
                            all.as_slice()
                        }
                    };
                    for &i in shown {
                        let offset = (i as u64 * banished_gfx::skin::PALETTE_BYTES) as u32;
                        pass.set_bind_group(2, skin, &[offset]);
                        pass.draw_indexed(0..r.index_count, 0, i..i + 1);
                    }
                }
            }
        }
    }
}
