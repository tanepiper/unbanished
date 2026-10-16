//! Drawing particles: camera-facing quads lit by the sprite's normal map (`ParticleMaterial`),
//! or quads lying in the ground plane, unlit (`ParticleWaterMaterial`). The shaders follow the
//! materials' D3D9 programs; see docs/PARTICLES.md.

use std::collections::HashMap;

use banished_formats::types::ImageBuffer;
use wgpu::util::DeviceExt;

use crate::particles::ParticleBatch;
use crate::scene_gfx::{DEPTH_FORMAT, GLOBALS_WGSL, SCENE_FORMAT};

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
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let rgba = |label, pixel: [u8; 4]| {
            banished_gfx::upload_rgba_as(
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
            sampler: crate::scene_gfx::sampler(device, wgpu::AddressMode::ClampToEdge),
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
                        banished_gfx::upload_image_linear(device, queue, "particle texture", i)
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

const WEATHER_WGSL: &str = r#"
@group(1) @binding(0) var diffuse: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;
@group(1) @binding(2) var<uniform> wp: WeatherParams;

struct WeatherParams {
    // near0, near1, far0, far1
    fade: vec4<f32>,
    // half size, streaks, alpha
    misc: vec4<f32>,
};

struct WIn {
    @location(0) pos: vec3<f32>,
    @location(1) prev: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) tile: vec3<f32>,
};

struct WOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) fade: f32,
};

// WeatherGeometry's quads (`FUN_1400d85f0`): snow a camera-facing square, rain a streak from
// the last position to this one; WeatherMaterial_vs fades the alpha with the depth.
@vertex
fn vs_weather(v: WIn) -> WOut {
    var o: WOut;
    let h = wp.misc.x;
    let side = v.uv.x * 2.0 - 1.0;
    var world: vec3<f32>;
    if (wp.misc.y > 0.5) {
        var along = v.pos - v.prev;
        if (length(along) < 1e-5) {
            along = vec3<f32>(0.0, 0.0, -1.0);
        }
        let w = normalize(cross(along, g.camera_forward.xyz)) * h;
        let end = select(v.prev, v.pos, v.uv.y < 0.5);
        world = end + side * w;
    } else {
        let r = g.camera_right.xyz * h;
        let u = g.camera_up.xyz * h;
        let end = select(-r, r, v.uv.y < 0.5);
        world = v.pos + end + side * u;
    }
    world = world + v.tile;
    o.clip = g.view_proj * vec4<f32>(world, 1.0);
    o.uv = v.uv;
    let d = o.clip.w;
    let near = clamp((d - wp.fade.x) / (wp.fade.y - wp.fade.x), 0.0, 1.0);
    let far = clamp((d - wp.fade.z) / (wp.fade.w - wp.fade.z), 0.0, 1.0);
    o.fade = near * far * wp.misc.z;
    return o;
}

// WeatherMaterial_ps.
@fragment
fn fs_weather(i: WOut) -> @location(0) vec4<f32> {
    let t = textureSample(diffuse, samp, i.uv);
    return vec4<f32>(t.rgb, t.a * i.fade);
}
"#;

struct WeatherGpu {
    bind: wgpu::BindGroup,
    params: wgpu::Buffer,
    vertices: wgpu::Buffer,
    tiles: wgpu::Buffer,
    quads: u32,
    tile_count: u32,
    capacity: (usize, usize),
}

/// Rain and snow: each slot's quads drawn once per visible tile.
pub struct WeatherRenderer {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    white: wgpu::TextureView,
    indices: Option<(wgpu::Buffer, usize)>,
    slots: HashMap<String, WeatherGpu>,
    drawn: Vec<String>,
}

impl WeatherRenderer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        globals_layout: &wgpu::BindGroupLayout,
    ) -> WeatherRenderer {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("weather"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("weather"),
            source: wgpu::ShaderSource::Wgsl(format!("{GLOBALS_WGSL}{WEATHER_WGSL}").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("weather"),
            bind_group_layouts: &[Some(globals_layout), Some(&layout)],
            immediate_size: 0,
        });
        let vertex_attrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2];
        let tile_attrs = wgpu::vertex_attr_array![3 => Float32x3];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("weather"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_weather"),
                compilation_options: Default::default(),
                buffers: &[
                    Some(wgpu::VertexBufferLayout {
                        array_stride: 32,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &vertex_attrs,
                    }),
                    Some(wgpu::VertexBufferLayout {
                        array_stride: 12,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &tile_attrs,
                    }),
                ],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_weather"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: SCENE_FORMAT,
                    blend: Some(wgpu::BlendState {
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
                    }),
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
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        WeatherRenderer {
            pipeline,
            layout,
            sampler: crate::scene_gfx::sampler(device, wgpu::AddressMode::ClampToEdge),
            white: banished_gfx::upload_rgba_as(
                device,
                queue,
                "white",
                1,
                1,
                &[vec![255; 4]],
                wgpu::TextureFormat::Rgba8Unorm,
            ),
            indices: None,
            slots: HashMap::new(),
            drawn: Vec::new(),
        }
    }

    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        batches: &[crate::weather::WeatherBatch],
        eye: glam::Vec3,
        view_proj: &glam::Mat4,
        mut texture: impl FnMut(&str) -> Option<ImageBuffer>,
    ) {
        self.drawn.clear();
        for b in batches {
            let quads = b.vertices.len() / 32;
            let tiles = crate::weather::tiles(b.size, eye, view_proj);
            if quads == 0 || tiles.is_empty() {
                continue;
            }
            if self.indices.as_ref().is_none_or(|(_, n)| *n < quads) {
                let n = quads.next_power_of_two();
                // `FUN_1400d9450`'s quads: 0, 1, 2, 0, 2, 3.
                let indices: Vec<u32> = (0..n as u32)
                    .flat_map(|q| [0, 1, 2, 0, 2, 3].map(|i| q * 4 + i))
                    .collect();
                let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("weather indices"),
                    contents: bytemuck::cast_slice(&indices),
                    usage: wgpu::BufferUsages::INDEX,
                });
                self.indices = Some((buffer, n));
            }
            let fits = |s: &WeatherGpu| quads <= s.capacity.0 && tiles.len() <= s.capacity.1;
            if !self.slots.get(&b.material).is_some_and(fits) {
                let view = texture(&b.material)
                    .and_then(|i| {
                        banished_gfx::upload_image_linear(device, queue, "weather texture", &i)
                    })
                    .unwrap_or_else(|| self.white.clone());
                let params = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("weather params"),
                    size: 32,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("weather"),
                    layout: &self.layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&self.sampler),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: params.as_entire_binding(),
                        },
                    ],
                });
                let capacity = (quads.next_power_of_two(), 125);
                let vertices = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("weather vertices"),
                    size: (capacity.0 * 4 * 32) as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                let tiles = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("weather tiles"),
                    size: (capacity.1 * 12) as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                self.slots.insert(
                    b.material.clone(),
                    WeatherGpu {
                        bind,
                        params,
                        vertices,
                        tiles,
                        quads: 0,
                        tile_count: 0,
                        capacity,
                    },
                );
            }
            let s = self.slots.get_mut(&b.material).unwrap();
            let params: [f32; 8] = [
                b.fade[0],
                b.fade[1],
                b.fade[2],
                b.fade[3],
                b.half,
                if b.streaks { 1.0 } else { 0.0 },
                b.alpha,
                0.0,
            ];
            queue.write_buffer(&s.params, 0, bytemuck::cast_slice(&params));
            queue.write_buffer(&s.vertices, 0, bytemuck::cast_slice(&b.vertices));
            let flat: Vec<f32> = tiles.iter().flatten().copied().collect();
            queue.write_buffer(&s.tiles, 0, bytemuck::cast_slice(&flat));
            s.quads = quads as u32;
            s.tile_count = tiles.len() as u32;
            self.drawn.push(b.material.clone());
        }
    }

    pub fn draw(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        globals: &wgpu::BindGroup,
    ) {
        let Some((indices, _)) = &self.indices else {
            return;
        };
        if self.drawn.is_empty() {
            return;
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("weather"),
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
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, globals, &[]);
        pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
        for m in &self.drawn {
            let Some(s) = self.slots.get(m) else { continue };
            pass.set_bind_group(1, &s.bind, &[]);
            pass.set_vertex_buffer(0, s.vertices.slice(..));
            pass.set_vertex_buffer(1, s.tiles.slice(..));
            pass.draw_indexed(0..s.quads * 6, 0, 0..s.tile_count);
        }
    }
}
