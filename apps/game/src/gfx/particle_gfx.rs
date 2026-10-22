//! Drawing particles: camera-facing quads lit by the sprite's normal map (`ParticleMaterial`),
//! or quads lying in the ground plane, unlit (`ParticleWaterMaterial`). The shaders follow the
//! materials' D3D9 programs; see docs/PARTICLES.md.

mod weather;
pub use weather::WeatherRenderer;

use std::collections::HashMap;

use unbanished_formats::types::ImageBuffer;
use wgpu::util::DeviceExt;

use crate::gfx::scene_gfx::{DEPTH_FORMAT, GLOBALS_WGSL, SCENE_FORMAT};
use crate::view::particles::ParticleBatch;

const PARTICLE_WGSL: &str = r#"
@group(1) @binding(0) var diffuse: texture_2d<f32>;
@group(1) @binding(1) var normal_map: texture_2d<f32>;
@group(1) @binding(2) var samp: sampler;

struct PIn {
    @location(0) pos: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) corner: vec2<f32>,
    @location(4) rot: f32,
};

struct POut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) light: vec3<f32>,
    @location(2) color: vec4<f32>,
    @location(3) fog: f32,
};

// ParticleMaterial_vs: the corner turned by the rotation along the camera's right and up; the
// sprite's light a fixed direction turned with it.
@vertex
fn vs_lit(v: PIn) -> POut {
    var o: POut;
    let c = cos(v.rot);
    let s = sin(v.rot);
    let ox = c * v.corner.x - s * v.corner.y;
    let oy = c * v.corner.y + s * v.corner.x;
    let world = v.pos + ox * g.camera_right.xyz + oy * g.camera_up.xyz;
    o.clip = g.view_proj * vec4<f32>(world, 1.0);
    o.uv = v.uv;
    o.light = vec3<f32>(0.632436454 * (c - s), 0.632436454 * (s + c), 0.447267622);
    o.color = v.color;
    o.fog = (o.clip.w + g.fog.x) * g.fog.y;
    return o;
}

// ParticleMaterial_ps: the normal from the map's alpha and green, lit by the ambient and the
// light colours, then fogged.
@fragment
fn fs_lit(i: POut) -> @location(0) vec4<f32> {
    let nm = textureSample(normal_map, samp, i.uv);
    let nxy = vec2<f32>(nm.w * 2.0 - 1.0, nm.y * 2.0 - 1.0);
    let n = vec3<f32>(nxy, 1.0 - length(nxy));
    let d = dot(n, i.light) * 2.0;
    let amb = clamp((d * d + 1.0) * 0.5, 0.0, 1.0);
    let lit = amb * g.ambient_color.rgb + clamp(d * d * g.light_color.rgb, vec3<f32>(0.0), vec3<f32>(1.0));
    let base = textureSample(diffuse, samp, i.uv) * i.color;
    let shade = lit * base.rgb;
    let f = min(g.fog_color.w, clamp(i.fog, 0.0, 1.0));
    return vec4<f32>(shade + f * (g.fog_color.rgb - shade), base.a);
}

// ParticleWaterMaterial_vs: the corner turned in the ground plane.
@vertex
fn vs_flat(v: PIn) -> POut {
    var o: POut;
    let c = cos(v.rot);
    let s = sin(v.rot);
    let ox = c * v.corner.x - s * v.corner.y;
    let oy = c * v.corner.y + s * v.corner.x;
    let world = v.pos + vec3<f32>(ox, oy, 0.0);
    o.clip = g.view_proj * vec4<f32>(world, 1.0);
    o.uv = v.uv;
    o.light = vec3<f32>(0.0);
    o.color = v.color;
    o.fog = 0.0;
    return o;
}

@fragment
fn fs_flat(i: POut) -> @location(0) vec4<f32> {
    return textureSample(diffuse, samp, i.uv) * i.color;
}
"#;

/// Floats a vertex: position, colour and alpha, uv, corner, rotation.
const FLOATS: usize = 12;

struct Batch {
    material: String,
    flat: bool,
    first: u32,
    count: u32,
}

pub struct ParticleRenderer {
    lit: wgpu::RenderPipeline,
    flat: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    white: wgpu::TextureView,
    flat_normal: wgpu::TextureView,
    materials: HashMap<String, wgpu::BindGroup>,
    vertices: Option<wgpu::Buffer>,
    indices: Option<wgpu::Buffer>,
    quads: usize,
    batches: Vec<Batch>,
}

impl ParticleRenderer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        globals_layout: &wgpu::BindGroupLayout,
    ) -> ParticleRenderer {
        let tex = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("particle material"),
            entries: &[
                tex(0),
                tex(1),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("particles"),
            source: wgpu::ShaderSource::Wgsl(format!("{GLOBALS_WGSL}{PARTICLE_WGSL}").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("particles"),
            bind_group_layouts: &[Some(globals_layout), Some(&layout)],
            immediate_size: 0,
        });
        let attrs = wgpu::vertex_attr_array![
            0 => Float32x3, 1 => Float32x4, 2 => Float32x2, 3 => Float32x2, 4 => Float32
        ];
        // `ParticleMaterial_rs`: blended by alpha, depth tested, not written. The scene's alpha
        // (the water's underwater factor) is kept.
        let blend = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        };
        let make = |vs: &str, fs: &str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(vs),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vs),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: (FLOATS * 4) as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &attrs,
                    })],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: SCENE_FORMAT,
                        blend: Some(blend),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: crate::gfx::scene_gfx::multisample(),
                multiview_mask: None,
                cache: None,
            })
        };
        let rgba = |label, pixel: [u8; 4]| {
            unbanished_gfx::upload_rgba_as(
                device,
                queue,
                label,
                1,
                1,
                &[pixel.to_vec()],
                wgpu::TextureFormat::Rgba8Unorm,
            )
        };
        ParticleRenderer {
            lit: make("vs_lit", "fs_lit"),
            flat: make("vs_flat", "fs_flat"),
            layout,
            sampler: crate::gfx::scene_gfx::sampler(device, wgpu::AddressMode::ClampToEdge),
            white: rgba("white", [255; 4]),
            // A flat normal: x and y (alpha, green) at the middle.
            flat_normal: rgba("flat normal", [128, 128, 255, 128]),
            materials: HashMap::new(),
            vertices: None,
            indices: None,
            quads: 0,
            batches: Vec::new(),
        }
    }

    /// Upload this frame's particles; `textures` gives a new material's diffuse and normal map.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        batches: &[ParticleBatch],
        mut textures: impl FnMut(&str) -> (Option<ImageBuffer>, Option<ImageBuffer>),
    ) {
        self.batches.clear();
        let quads: usize = batches
            .iter()
            .map(|b| b.vertices.len() / (FLOATS * 4))
            .sum();
        if quads == 0 {
            return;
        }
        for b in batches {
            if self.materials.contains_key(&b.material) {
                continue;
            }
            let (d, n) = textures(&b.material);
            let view = |img: &Option<ImageBuffer>, fallback: &wgpu::TextureView| {
                img.as_ref()
                    .and_then(|i| {
                        unbanished_gfx::upload_image_linear(device, queue, "particle texture", i)
                    })
                    .unwrap_or_else(|| fallback.clone())
            };
            let (dv, nv) = (view(&d, &self.white), view(&n, &self.flat_normal));
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("particle material"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&dv),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&nv),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            });
            self.materials.insert(b.material.clone(), bind);
        }
        if self.quads < quads {
            self.quads = quads.next_power_of_two();
            self.vertices = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("particle vertices"),
                size: (self.quads * 4 * FLOATS * 4) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
            // `ParticleInstance::Draw`'s quads: 0, 2, 1, 0, 3, 2.
            let indices: Vec<u32> = (0..self.quads as u32)
                .flat_map(|q| [0, 2, 1, 0, 3, 2].map(|i| q * 4 + i))
                .collect();
            self.indices = Some(
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("particle indices"),
                    contents: bytemuck::cast_slice(&indices),
                    usage: wgpu::BufferUsages::INDEX,
                }),
            );
        }
        let mut data: Vec<f32> = Vec::with_capacity(quads * 4 * FLOATS);
        for b in batches {
            let first = (data.len() / (FLOATS * 4)) as u32;
            data.extend_from_slice(&b.vertices);
            let count = (b.vertices.len() / (FLOATS * 4)) as u32;
            self.batches.push(Batch {
                material: b.material.clone(),
                flat: b.flat,
                first,
                count,
            });
        }
        if let Some(v) = &self.vertices {
            queue.write_buffer(v, 0, bytemuck::cast_slice(&data));
        }
    }

    pub fn draw(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        globals: &wgpu::BindGroup,
    ) {
        let (Some(vertices), Some(indices)) = (&self.vertices, &self.indices) else {
            return;
        };
        if self.batches.is_empty() {
            return;
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("particles"),
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
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_bind_group(0, globals, &[]);
        pass.set_vertex_buffer(0, vertices.slice(..));
        pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
        for b in &self.batches {
            let Some(bind) = self.materials.get(&b.material) else {
                continue;
            };
            pass.set_pipeline(if b.flat { &self.flat } else { &self.lit });
            pass.set_bind_group(1, bind, &[]);
            pass.draw_indexed(b.first * 6..(b.first + b.count) * 6, 0, 0..1);
        }
    }
}
