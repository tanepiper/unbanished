//! Draws `banished_ui` quads: one alpha-blended pipeline, batches split by texture and scissor.

use std::collections::HashMap;

use banished_formats::texture::decode_rgba;
use banished_formats::types;
use banished_ui::{Quad, Texture, Ui};

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    pos: [f32; 2],
    uv: [f32; 2],
    color: [u8; 4],
}

const SHADER: &str = r#"
struct Screen { size: vec4<f32> };
@group(0) @binding(0) var<uniform> screen: Screen;
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(@location(0) pos: vec2<f32>, @location(1) uv: vec2<f32>, @location(2) color: vec4<f32>) -> VsOut {
    var out: VsOut;
    out.clip = vec4<f32>(pos.x / screen.size.x * 2.0 - 1.0, 1.0 - pos.y / screen.size.y * 2.0, 0.0, 1.0);
    out.uv = uv;
    out.color = color;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return textureSample(tex, samp, in.uv) * in.color;
}
"#;

pub struct UiRenderer {
    pipeline: wgpu::RenderPipeline,
    screen: wgpu::Buffer,
    screen_group: wgpu::BindGroup,
    texture_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    textures: HashMap<Texture, wgpu::BindGroup>,
    vertices: Option<wgpu::Buffer>,
}

struct Batch {
    texture: Texture,
    clip: Option<[i32; 4]>,
    range: std::ops::Range<u32>,
}

impl UiRenderer {
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        depth: wgpu::TextureFormat,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ui"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let uniform_entry = wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let screen_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui screen"),
            entries: &[uniform_entry],
        });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui texture"),
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
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ui"),
            bind_group_layouts: &[Some(&screen_layout), Some(&texture_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ui"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Unorm8x4],
                })],
            },
            primitive: Default::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: depth,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let screen = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ui screen"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let screen_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ui screen"),
            layout: &screen_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: screen.as_entire_binding(),
            }],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        UiRenderer {
            pipeline,
            screen,
            screen_group,
            texture_layout,
            sampler,
            textures: HashMap::new(),
            vertices: None,
        }
    }

    fn bind(&self, device: &wgpu::Device, view: &wgpu::TextureView) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ui texture"),
            layout: &self.texture_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        })
    }

    /// Register an application texture for `Texture::External(id)`.
    pub fn set_external(&mut self, device: &wgpu::Device, id: u32, view: &wgpu::TextureView) {
        let group = self.bind(device, view);
        self.textures.insert(Texture::External(id), group);
    }

    /// Upload any sprite-sheet images the UI references that are not on the GPU yet.
    fn ensure_sheets(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, ui: &Ui) {
        for (i, sheet) in ui.res.sheets.iter().enumerate() {
            if self.textures.contains_key(&Texture::Sheet(i)) {
                continue;
            }
            let rgba = sheet.image.as_deref().and_then(|r| {
                let img = types::image_buffer(&ui.res.archive.read_ref(r).ok()?).ok()?;
                let data = decode_rgba(img.format, img.width, img.height, img.levels.first()?)?;
                Some((img.width, img.height, data))
            });
            let (w, h, data) = rgba.unwrap_or_else(|| {
                log::warn!("sprite sheet {} has no readable image", sheet.reference);
                (1, 1, vec![255; 4])
            });
            let view = crate::render::upload_rgba(device, queue, &sheet.reference, w, h, &[data]);
            let group = self.bind(device, &view);
            self.textures.insert(Texture::Sheet(i), group);
        }
    }

    pub fn draw<'p>(
        &'p mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pass: &mut wgpu::RenderPass<'p>,
        ui: &Ui,
        quads: &[Quad],
        size: (u32, u32),
    ) {
        self.ensure_sheets(device, queue, ui);
        queue.write_buffer(
            &self.screen,
            0,
            bytemuck::cast_slice(&[size.0 as f32, size.1 as f32, 0.0, 0.0]),
        );
        let mut vertices = Vec::with_capacity(quads.len() * 6);
        let mut batches: Vec<Batch> = Vec::new();
        for q in quads {
            let start = vertices.len() as u32;
            let [x0, y0, x1, y1] = q.rect;
            let [u0, v0, u1, v1] = q.uv;
            let v = |x, y, u, v| Vertex {
                pos: [x, y],
                uv: [u, v],
                color: q.color,
            };
            vertices.extend_from_slice(&[
                v(x0, y0, u0, v0),
                v(x1, y0, u1, v0),
                v(x1, y1, u1, v1),
                v(x0, y0, u0, v0),
                v(x1, y1, u1, v1),
                v(x0, y1, u0, v1),
            ]);
            match batches.last_mut() {
                Some(b) if b.texture == q.texture && b.clip == q.clip => b.range.end += 6,
                _ => batches.push(Batch {
                    texture: q.texture,
                    clip: q.clip,
                    range: start..start + 6,
                }),
            }
        }
        if vertices.is_empty() {
            return;
        }
        let bytes: &[u8] = bytemuck::cast_slice(&vertices);
        let needs_new = self
            .vertices
            .as_ref()
            .is_none_or(|b| b.size() < bytes.len() as u64);
        if needs_new {
            self.vertices = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("ui vertices"),
                size: (bytes.len() as u64).next_power_of_two(),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }
        let buffer = self.vertices.as_ref().unwrap();
        queue.write_buffer(buffer, 0, bytes);
        pass.set_viewport(0.0, 0.0, size.0 as f32, size.1 as f32, 0.0, 1.0);
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.screen_group, &[]);
        pass.set_vertex_buffer(0, buffer.slice(..));
        for b in batches {
            let Some(group) = self.textures.get(&b.texture) else {
                continue;
            };
            let [x0, y0, x1, y1] = b.clip.unwrap_or([0, 0, size.0 as i32, size.1 as i32]);
            let (x0, y0) = (
                x0.clamp(0, size.0 as i32) as u32,
                y0.clamp(0, size.1 as i32) as u32,
            );
            let (x1, y1) = (
                x1.clamp(0, size.0 as i32) as u32,
                y1.clamp(0, size.1 as i32) as u32,
            );
            if x1 <= x0 || y1 <= y0 {
                continue;
            }
            pass.set_scissor_rect(x0, y0, x1 - x0, y1 - y0);
            pass.set_bind_group(1, group, &[]);
            pass.draw(b.range, 0..1);
        }
        pass.set_scissor_rect(0, 0, size.0, size.1);
    }
}
