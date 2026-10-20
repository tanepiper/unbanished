//! Draws `unbanished_ui` quads: one alpha-blended pipeline, batches split by texture and scissor.

use std::collections::HashMap;

use unbanished_formats::texture::decode_rgba;
use unbanished_formats::types;
use unbanished_ui::{Quad, Texture, Ui};

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

// `Material\UI\Logo` (the Shining Rock splash), from the D3D9 pixel shader: red is the logo, green
// where rays show, blue a ray pattern sampled after rotating by `userPosition.x` (seconds) about
// texel (800, 421) of the 1600x900 image. Dark logo on white.
@fragment
fn fs_logo(in: VsOut) -> @location(0) vec4<f32> {
    let base = textureSample(tex, samp, in.uv);
    let a = screen.size.z;
    let s = sin(a);
    let c = cos(a);
    let p = in.uv * vec2<f32>(1600.0, 900.0) + vec2<f32>(-800.0, -421.0);
    let r = vec2<f32>(c * p.x + s * p.y + 800.0, -s * p.x + c * p.y + 421.0);
    let rays = textureSample(tex, samp, r * vec2<f32>(0.000624999986, 0.00111111114));
    let i = base.r + base.g * (1.0 - rays.b);
    return vec4<f32>(vec3<f32>(1.0) - i * vec3<f32>(0.831372559, 0.792156875, 0.752941191), in.color.a);
}
"#;

/// Draws the quads of `unbanished-ui` with wgpu: one alpha-blended pipeline (and one for the logo
/// material), in batches split by texture and clip rectangle. It uploads each sprite sheet the
/// first time a quad uses it.
pub struct UiRenderer {
    pipeline: wgpu::RenderPipeline,
    logo_pipeline: wgpu::RenderPipeline,
    /// Sheets drawn with the Logo material.
    logo_sheets: std::collections::HashSet<Texture>,
    /// `pc_userPosition.x` for material shaders (seconds since the loader loop started).
    time: f32,
    screen: wgpu::Buffer,
    screen_group: wgpu::BindGroup,
    texture_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    textures: HashMap<Texture, wgpu::BindGroup>,
    vertices: Option<wgpu::Buffer>,
    /// Sprite sheets are sRGB when the target is, so colours come out unchanged either way.
    sheet_format: wgpu::TextureFormat,
}

struct Batch {
    texture: Texture,
    clip: Option<[i32; 4]>,
    range: std::ops::Range<u32>,
}

impl UiRenderer {
    /// A renderer for a colour target of `format` and a depth target of `depth`.
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
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
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
        let make = |entry: &str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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
                entry_point: Some(entry),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        })
        };
        let pipeline = make("fs_main");
        let logo_pipeline = make("fs_logo");
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
            logo_pipeline,
            logo_sheets: Default::default(),
            time: 0.0,
            screen,
            screen_group,
            texture_layout,
            sampler,
            textures: HashMap::new(),
            vertices: None,
            sheet_format: if format.is_srgb() {
                wgpu::TextureFormat::Rgba8UnormSrgb
            } else {
                wgpu::TextureFormat::Rgba8Unorm
            },
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

    /// Forget the sprite sheets' textures (the UI's resources were made again, so its sheet
    /// numbers name other sheets now).
    pub fn clear_sheets(&mut self) {
        self.textures.retain(|t, _| !matches!(t, Texture::Sheet(_)));
        self.logo_sheets.clear();
    }

    /// Upload any sprite-sheet images the UI references that are not on the GPU yet.
    fn ensure_sheets(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, ui: &Ui) {
        for (i, sheet) in ui.res.sheets.iter().enumerate() {
            if self.textures.contains_key(&Texture::Sheet(i)) {
                continue;
            }
            let made = sheet.pixels.as_deref().map(|(w, h, p)| (*w, *h, p.clone()));
            let rgba = made.or_else(|| {
                sheet.image.as_deref().and_then(|r| {
                    let img = types::image_buffer(&ui.res.archive.read_ref(r).ok()?).ok()?;
                    let data = decode_rgba(img.format, img.width, img.height, img.levels.first()?)?;
                    Some((img.width, img.height, data))
                })
            });
            let (w, h, data) = rgba.unwrap_or_else(|| {
                log::warn!("sprite sheet {} has no readable image", sheet.reference);
                (1, 1, vec![255; 4])
            });
            let view = crate::upload_rgba_as(
                device,
                queue,
                &sheet.reference,
                w,
                h,
                &[data],
                self.sheet_format,
            );
            let group = self.bind(device, &view);
            self.textures.insert(Texture::Sheet(i), group);
            if sheet
                .shader
                .as_deref()
                .is_some_and(|m| m.replace('/', "\\").contains("Material\\UI\\Logo"))
            {
                self.logo_sheets.insert(Texture::Sheet(i));
            }
        }
    }

    /// Time for animated UI materials (`pc_userPosition.x`).
    pub fn set_time(&mut self, seconds: f32) {
        self.time = seconds;
    }

    /// Draw `quads` (from [`Ui::draw`](unbanished_ui::Ui::draw)) into `pass`, on a target of `size`
    /// pixels.
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
            bytemuck::cast_slice(&[size.0 as f32, size.1 as f32, self.time, 0.0]),
        );
        let mut vertices = Vec::with_capacity(quads.len() * 6);
        let mut batches: Vec<Batch> = Vec::new();
        for q in quads {
            let start = vertices.len() as u32;
            let [x0, y0, x1, y1] = q.rect;
            let [u0, v0, u1, v1] = q.uv;
            let (sin, cos) = q.rotation.to_radians().sin_cos();
            let (cx, cy) = ((x0 + x1) * 0.5, (y0 + y1) * 0.5);
            let v = |x: f32, y: f32, u, v| {
                let (dx, dy) = (x - cx, y - cy);
                Vertex {
                    pos: [cx + dx * cos - dy * sin, cy + dx * sin + dy * cos],
                    uv: [u, v],
                    color: q.color,
                }
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
        pass.set_bind_group(0, &self.screen_group, &[]);
        pass.set_vertex_buffer(0, buffer.slice(..));
        let mut logo = None;
        for b in batches {
            let is_logo = self.logo_sheets.contains(&b.texture);
            if logo != Some(is_logo) {
                pass.set_pipeline(if is_logo {
                    &self.logo_pipeline
                } else {
                    &self.pipeline
                });
                logo = Some(is_logo);
            }
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
