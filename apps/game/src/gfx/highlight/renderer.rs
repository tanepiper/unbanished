//! Drawing the highlight's edges: the shader and its pipeline.

use super::*;

pub(super) const SHADER: &str = r#"
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
                        format: crate::gfx::scene_gfx::SCENE_FORMAT,
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
                    format: crate::gfx::scene_gfx::DEPTH_FORMAT,
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
                multisample: crate::gfx::scene_gfx::multisample(),
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
        frame: &crate::gfx::render::SceneFrame,
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
