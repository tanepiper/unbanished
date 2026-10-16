//! HighlightComponent 14004ce00 and Runtime EdgeMeshInstance 1800a3f20/1800a40a0.
use crate::{selection::Entity, start::StartingTown};
use banished_formats::{
    picking::{self, EdgeMesh, HighlightDescription},
    pkg::Archive,
    skeleton::{self, Transform},
    types::{self, GraphicsMesh},
};
use glam::Vec3;
use std::collections::BTreeMap;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct EdgeVertex {
    a: [f32; 3],
    corner: f32,
    b: [f32; 3],
    _pad: f32,
}

#[derive(Default, Clone, PartialEq)]
pub struct Geometry {
    mask: Vec<[f32; 3]>,
    edges: Vec<EdgeVertex>,
}

impl Geometry {
    pub fn is_empty(&self) -> bool {
        self.mask.is_empty() || self.edges.is_empty()
    }

    #[cfg(test)]
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }
}

#[derive(Default)]
pub struct Highlight {
    descriptions: BTreeMap<String, Option<HighlightDescription>>,
    edges: BTreeMap<String, Option<EdgeMesh>>,
    models: BTreeMap<String, Option<GraphicsMesh>>,
}

fn position(bytes: &[u8]) -> [f32; 3] {
    std::array::from_fn(|i| {
        i16::from_le_bytes(bytes[i * 2..i * 2 + 2].try_into().unwrap()) as f32 / 512.
    })
}

impl Highlight {
    pub fn geometry(
        &mut self,
        archive: &Archive,
        town: &StartingTown,
        grid: &banished_game::terrain::HeightGrid<'_>,
        entity: Option<Entity>,
    ) -> Geometry {
        let mut out = Geometry::default();
        let Some(entity) = entity else {
            return out;
        };
        // `FUN_140101eb0`: a MultiObject's parts (`+0x48`) light up with it.
        if let Entity::Placed(e) = entity {
            let parts = town
                .construction
                .sites
                .get(&e)
                .map(|s| s.parts.clone())
                .unwrap_or_default();
            for p in parts {
                if town.placed.get(p).is_some_and(|x| x.alive) {
                    self.append(archive, town, grid, Entity::Placed(p), &mut out);
                }
            }
        }
        self.append(archive, town, grid, entity, &mut out);
        out
    }

    /// One entity's highlight (`FUN_14004ce00`), added to `out`.
    fn append(
        &mut self,
        archive: &Archive,
        town: &StartingTown,
        grid: &banished_game::terrain::HeightGrid<'_>,
        entity: Entity,
        out: &mut Geometry,
    ) {
        let key = entity
            .template(town)
            .split(':')
            .next()
            .unwrap_or("")
            .replace('\\', "/");
        let Some(desc) = self
            .descriptions
            .entry(key.clone())
            .or_insert_with(|| {
                archive
                    .read_ref(&format!("{key}:highlight"))
                    .ok()
                    .and_then(|b| picking::highlight_description(&b).ok())
            })
            .as_ref()
        else {
            return;
        };
        let (variant, stage) = match entity {
            Entity::Placed(i) => (town.placed[i].variant, town.placed[i].mesh),
            Entity::Citizen(i) => (usize::from(!town.citizens[i].male), 0),
        };
        let reference = desc
            .meshes
            .get(variant)
            .and_then(|v| v.get(stage))
            .and_then(Option::as_deref);
        let Some(reference) = reference else {
            // FUN_14004c290: ground triangles inside the rectangle, and four perimeter runs.
            if let Entity::Placed(i) = entity {
                let p = &town.placed[i];
                let (x, y) = p.cell;
                let (w, h) = p.footprint;
                let half = town.cells.size as i32 / 2;
                let offset = (grid.width as i32 - 1) / 2 - half;
                let corner = |x: i32, y: i32| {
                    let v = grid
                        .vertex(x + offset, y + offset, grid.width as i32 - 1)
                        .pos;
                    [v[0] as f32, v[1] as f32, v[2] as f32 / 256.]
                };
                let edge = |out: &mut Geometry, a: [f32; 3], b: [f32; 3]| {
                    let v = [
                        EdgeVertex {
                            a,
                            corner: 1.,
                            b,
                            _pad: 0.,
                        },
                        EdgeVertex {
                            a,
                            corner: -1.,
                            b,
                            _pad: 0.,
                        },
                        EdgeVertex {
                            a: b,
                            corner: 1.,
                            b: a,
                            _pad: 0.,
                        },
                        EdgeVertex {
                            a: b,
                            corner: -1.,
                            b: a,
                            _pad: 0.,
                        },
                    ];
                    for k in [0, 1, 2, 0, 2, 3] {
                        out.edges.push(v[k]);
                    }
                };
                for cx in x..x + w {
                    edge(out, corner(cx, y), corner(cx + 1, y));
                    edge(out, corner(cx, y + h), corner(cx + 1, y + h));
                }
                for cy in y..y + h {
                    edge(out, corner(x, cy), corner(x, cy + 1));
                    edge(out, corner(x + w, cy), corner(x + w, cy + 1));
                }
                for cy in y..y + h {
                    for cx in x..x + w {
                        let v = [
                            corner(cx, cy),
                            corner(cx + 1, cy),
                            corner(cx, cy + 1),
                            corner(cx + 1, cy + 1),
                        ];
                        for k in if (cx + cy) & 1 == 0 {
                            [0, 2, 1, 1, 2, 3]
                        } else {
                            [0, 3, 1, 0, 2, 3]
                        } {
                            out.mask.push(v[k]);
                        }
                    }
                }
            }
            return;
        };
        let Some(edge) = self
            .edges
            .entry(reference.to_owned())
            .or_insert_with(|| {
                archive
                    .read_ref(reference)
                    .ok()
                    .and_then(|b| picking::edge_mesh(&b).ok())
            })
            .as_ref()
        else {
            return;
        };
        let Some((model_ref, placement)) = entity.model(town) else {
            return;
        };
        let Some(model) = self
            .models
            .entry(model_ref.to_owned())
            .or_insert_with(|| {
                archive
                    .read_ref(model_ref)
                    .ok()
                    .and_then(|b| types::graphics_mesh(&b).ok())
            })
            .as_ref()
        else {
            return;
        };
        let mut locals = match entity {
            Entity::Citizen(i) => town
                .citizen_anims
                .get(variant)
                .and_then(Option::as_ref)
                .and_then(|g| {
                    let p = &town.agents.characters[i].animation;
                    g.clips.get(p.clip).map(|c| p.pose(c))
                }),
            Entity::Placed(i) => town.placed[i].animation.as_ref().and_then(|p| {
                town.templates[town.placed[i].template]
                    .animations
                    .get(variant)
                    .and_then(Option::as_ref)
                    .and_then(|g| g.clips.get(p.clip))
                    .map(|c| p.pose(c))
            }),
        }
        .unwrap_or_else(|| model.bones.iter().map(skeleton::local_bind).collect());
        if let Some(root) = locals.first_mut() {
            *root = Transform::IDENTITY;
        }
        let nodes = skeleton::world_transforms(&model.bones, &locals);
        let palette = skeleton::palette(&model.bones, &locals);
        // CPU skinning keeps the same four weighted point transforms; the render pass receives
        // world-space geometry and consumes no animation time or simulation state.
        let transform = |p: [f32; 3], node: i32, skin: Option<(&[u8], &[u8])>| {
            let p = if let Some((weights, indices)) = skin {
                let mut v = Vec3::ZERO;
                for k in 0..4 {
                    if let Some(t) = palette.get(indices[k] as usize) {
                        v += Vec3::from(t.apply(p)) * (weights[k] as f32 / 255.);
                    }
                }
                v
            } else {
                Vec3::from(
                    nodes
                        .get(node as usize)
                        .unwrap_or(&Transform::IDENTITY)
                        .apply(p),
                )
            };
            placement.transform_point3(p).to_array()
        };
        for (ranges, edge_pass) in [(&edge.ranges, false), (&edge.edge_ranges, true)] {
            for r in ranges {
                let skin = r[5] < 0;
                let stride = match (edge_pass, skin) {
                    (false, false) => 8,
                    (false, true) => 16,
                    (true, false) => 16,
                    (true, true) => 32,
                };
                let buffer = (if edge_pass { 4 } else { 0 }) + usize::from(skin) * 2;
                let indices = &edge.buffers[buffer];
                let vertices = &edge.buffers[buffer + 1];
                for j in r[2] as usize..(r[2] + r[3]) as usize {
                    let Some(index) = indices.get(j * 2..j * 2 + 2) else {
                        continue;
                    };
                    let i = r[0] as usize + u16::from_le_bytes(index.try_into().unwrap()) as usize;
                    let Some(v) = vertices.get(i * stride..(i + 1) * stride) else {
                        continue;
                    };
                    if edge_pass {
                        let a =
                            transform(position(v), r[5], skin.then(|| (&v[16..20], &v[20..24])));
                        let b = transform(
                            position(&v[8..]),
                            r[5],
                            skin.then(|| (&v[24..28], &v[28..32])),
                        );
                        let corner = i16::from_le_bytes(v[6..8].try_into().unwrap()) as f32 / 512.;
                        out.edges.push(EdgeVertex {
                            a,
                            corner,
                            b,
                            _pad: 0.,
                        });
                    } else {
                        out.mask.push(transform(
                            position(v),
                            r[5],
                            skin.then(|| (&v[8..12], &v[12..16])),
                        ));
                    }
                }
            }
        }
    }
}

const SHADER: &str = r#"
struct Uniforms { vp:mat4x4<f32>, right:vec4<f32>, up:vec4<f32> };
@group(0) @binding(0) var<uniform> u:Uniforms;
@vertex fn mask(@location(0) p:vec3<f32>)->@builtin(position) vec4<f32> {return u.vp*vec4<f32>(p,1.);}
// SelectionEdgeMaterial_Normal_vs program 0: camera-right projection differential gives
// world units per half pixel, four such units extend the edge ends and its two sides.
@vertex fn edge(@location(0) a:vec3<f32>,@location(1) corner:f32,@location(2) b:vec3<f32>)->@builtin(position) vec4<f32> {
    let p=u.vp*vec4<f32>(a,1.);let q=u.vp*vec4<f32>(a+u.right.xyz,1.);
    let width=(u.right.w/(q.x/q.w-p.x/p.w))*4.;
    let direction=normalize(b-a);let side=normalize(cross(direction,u.up.xyz));
    return u.vp*vec4<f32>(a-direction*width+side*(corner*width),1.);
}
@fragment fn colour()->@location(0) vec4<f32>{return vec4<f32>(0.99000001,0.800000012,0.300000012,1.);}
"#;

pub struct HighlightRenderer {
    mask: wgpu::RenderPipeline,
    edge: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
    bind: wgpu::BindGroup,
    geometry: Geometry,
    mask_buffer: Option<wgpu::Buffer>,
    edge_buffer: Option<wgpu::Buffer>,
}

impl HighlightRenderer {
    pub fn new(device: &wgpu::Device) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("selection"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("selection camera"),
            size: 96,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("selection camera"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("selection"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SelectionEdge/Mask"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        const EDGE_ATTRS: [wgpu::VertexAttribute; 3] =
            wgpu::vertex_attr_array![0=>Float32x3,1=>Float32,2=>Float32x3];
        const MASK_ATTRS: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![0=>Float32x3];
        let pipeline = |edge: bool| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(if edge {
                    "selection edges"
                } else {
                    "selection mask"
                }),
                layout: Some(&pl),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(if edge { "edge" } else { "mask" }),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: if edge { 32 } else { 12 },
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: if edge { &EDGE_ATTRS } else { &MASK_ATTRS },
                    })],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("colour"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: crate::scene_gfx::SCENE_FORMAT,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: if edge {
                            wgpu::ColorWrites::ALL
                        } else {
                            wgpu::ColorWrites::empty()
                        },
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    front_face: wgpu::FrontFace::Cw,
                    cull_mode: if edge { None } else { Some(wgpu::Face::Back) },
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: crate::scene_gfx::DEPTH_FORMAT,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(if edge {
                        wgpu::CompareFunction::Less
                    } else {
                        wgpu::CompareFunction::Always
                    }),
                    stencil: wgpu::StencilState {
                        front: wgpu::StencilFaceState {
                            compare: if edge {
                                wgpu::CompareFunction::Equal
                            } else {
                                wgpu::CompareFunction::Always
                            },
                            fail_op: wgpu::StencilOperation::Keep,
                            depth_fail_op: wgpu::StencilOperation::Keep,
                            pass_op: if edge {
                                wgpu::StencilOperation::Invert
                            } else {
                                wgpu::StencilOperation::Replace
                            },
                        },
                        back: if edge {
                            wgpu::StencilFaceState {
                                compare: wgpu::CompareFunction::Equal,
                                fail_op: wgpu::StencilOperation::Keep,
                                depth_fail_op: wgpu::StencilOperation::Keep,
                                pass_op: wgpu::StencilOperation::Invert,
                            }
                        } else {
                            wgpu::StencilFaceState::IGNORE
                        },
                        read_mask: 255,
                        write_mask: 255,
                    },
                    bias: if edge {
                        wgpu::DepthBiasState {
                            constant: -1677,
                            slope_scale: -0.4096,
                            clamp: 0.,
                        }
                    } else {
                        Default::default()
                    },
                }),
                multisample: crate::scene_gfx::multisample(),
                multiview_mask: None,
                cache: None,
            })
        };
        Self {
            mask: pipeline(false),
            edge: pipeline(true),
            uniform,
            bind,
            geometry: Geometry::default(),
            mask_buffer: None,
            edge_buffer: None,
        }
    }
    pub fn upload(&mut self, device: &wgpu::Device, geometry: &Geometry) {
        if &self.geometry == geometry {
            return;
        }
        let buffer = |label, data: &[u8]| {
            (!data.is_empty()).then(|| {
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(label),
                    contents: data,
                    usage: wgpu::BufferUsages::VERTEX,
                })
            })
        };
        self.mask_buffer = buffer("selection mask", bytemuck::cast_slice(&geometry.mask));
        self.edge_buffer = buffer("selection edge", bytemuck::cast_slice(&geometry.edges));
        self.geometry = geometry.clone();
    }
    pub fn draw(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        frame: &crate::render::SceneFrame,
        width: u32,
    ) {
        if self.geometry.is_empty() {
            return;
        }
        let (Some(mask), Some(edge)) = (&self.mask_buffer, &self.edge_buffer) else {
            return;
        };
        let mut values = Vec::from(frame.view_proj.to_cols_array());
        values.extend(frame.right.extend(1. / width as f32).to_array());
        values.extend(frame.up.extend(0.).to_array());
        queue.write_buffer(&self.uniform, 0, bytemuck::cast_slice(&values));
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("selection"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(0),
                    store: wgpu::StoreOp::Discard,
                }),
            }),
            ..Default::default()
        });
        pass.set_bind_group(0, &self.bind, &[]);
        pass.set_stencil_reference(1);
        pass.set_pipeline(&self.mask);
        pass.set_vertex_buffer(0, mask.slice(..));
        pass.draw(0..self.geometry.mask.len() as u32, 0..1);
        pass.set_stencil_reference(0);
        pass.set_pipeline(&self.edge);
        pass.set_vertex_buffer(0, edge.slice(..));
        pass.draw(0..self.geometry.edges.len() as u32, 0..1);
    }
}
