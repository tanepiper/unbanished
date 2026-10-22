//! Rain and snow: their shader, their slots, and drawing them once per visible tile.

use super::*;

pub(super) const WEATHER_WGSL: &str = r#"
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

pub(super) struct WeatherGpu {
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
            multisample: crate::gfx::scene_gfx::multisample(),
            multiview_mask: None,
            cache: None,
        });
        WeatherRenderer {
            pipeline,
            layout,
            sampler: crate::gfx::scene_gfx::sampler(device, wgpu::AddressMode::ClampToEdge),
            white: unbanished_gfx::upload_rgba_as(
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
        batches: &[crate::view::weather::WeatherBatch],
        eye: glam::Vec3,
        view_proj: &glam::Mat4,
        mut texture: impl FnMut(&str) -> Option<ImageBuffer>,
    ) {
        self.drawn.clear();
        for b in batches {
            let quads = b.vertices.len() / 32;
            let tiles = crate::view::weather::tiles(b.size, eye, view_proj);
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
                        unbanished_gfx::upload_image_linear(device, queue, "weather texture", &i)
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
