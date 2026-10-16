//! StatusIconComponent billboards (slice E): the component embeds a `Graphics::Geometry`
//! (vtable `0x1401654e0`) whose draw `FUN_1400af0e0` emits one camera-facing quad per set bit
//! of its mask. See docs/STATUS-ICONS.md. Building the quads reads only presentation state.

use std::collections::BTreeMap;

use glam::{Vec3, Vec4Swizzles};
use wgpu::util::DeviceExt;

/// One entity's icons (`StatusIconComponent` and its `StatusIconDescription`).
#[derive(Debug, Clone, PartialEq)]
pub struct StatusIcon {
    /// `FUN_1401310a0`: model position + model box height, or the map rectangle's centre on the
    /// ground + description `+0x2c` (always 1.0, never serialized).
    pub anchor: Vec3,
    /// Component `+0xf0` (geometry `+0xc8`).
    pub mask: u32,
    /// Description `+0x20`: the icon sprite sheet; sprite `i` draws bit `i`.
    pub sheet: String,
    /// Description `+0x28` (geometry `+0xc0`).
    pub size: f32,
    /// Description `+0x30` (geometry `+0xcc`): size is a fraction of the screen width.
    pub screen_constant: bool,
}

/// A loaded icon sheet: RGBA pixels and each sprite's `u0, v0, u1, v1` in sheet order
/// (`UI::SpriteSheet::GetUVs(index)`).
#[derive(Debug, Clone, PartialEq)]
pub struct IconSheet {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub uvs: Vec<[f32; 4]>,
}

/// Game options (`DAT_1401cbde0`, defaults set before `FUN_14009e030` loads them):
/// `StatusIconOpacity` `+0x8` (100), `StatusIconEnable` `+0xc` (true), `StatusIconScale` `+0x10`
/// (100).
#[derive(Debug, Clone, Copy)]
pub struct IconOptions {
    pub opacity: i32,
    pub enabled: bool,
    pub scale: i32,
}

impl Default for IconOptions {
    fn default() -> Self {
        IconOptions {
            opacity: 100,
            enabled: true,
            scale: 100,
        }
    }
}

/// Load an icon sprite sheet: its sprites' UVs and the material's `diffuse` image.
pub fn load_sheet(archive: &banished_formats::pkg::Archive, reference: &str) -> Option<IconSheet> {
    let r = reference.replace('\\', "/");
    let sheet = banished_formats::ui::sprite_sheet(&archive.read_ref(&r).ok()?).ok()?;
    let material = sheet.material.as_deref()?.replace('\\', "/");
    let mi = banished_formats::types::material_instance(&archive.read_ref(&material).ok()?).ok()?;
    let image = mi.texture("diffuse")?.replace('\\', "/");
    let img = banished_formats::types::image_buffer(&archive.read_ref(&image).ok()?).ok()?;
    let rgba = banished_formats::texture::decode_rgba(
        img.format,
        img.width,
        img.height,
        img.levels.first()?,
    )?;
    Some(IconSheet {
        width: img.width,
        height: img.height,
        rgba,
        uvs: sheet.sprites.iter().map(|s| s.uv).collect(),
    })
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct IconVertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
}

/// `FUN_1400af0e0`: the quads of one icon set, in world space.
pub fn quads(
    icon: &StatusIcon,
    uvs: &[[f32; 4]],
    view_proj: glam::Mat4,
    right: Vec3,
    up: Vec3,
    options: IconOptions,
) -> Vec<IconVertex> {
    let mut size = icon.size;
    if icon.screen_constant {
        // The anchor and anchor + right through the view-projection: size per unit of NDC x.
        let a = view_proj * icon.anchor.extend(1.0);
        let b = view_proj * (icon.anchor + right).extend(1.0);
        let (a, b) = (a.xyz() / a.w, b.xyz() / b.w);
        size /= (a.x - b.x).abs();
    }
    // `* (float)StatusIconScale * 0.01` (`DAT_1401787c4`).
    let size = size * options.scale as f32 * 0.01;
    let half = size * 0.5;
    let (hr, hu) = (right * half, up * half);
    // The first icon's centre is half a size above the anchor (world z); each next one a whole
    // size further along the camera's up axis.
    let mut p = Vec3::new(0.0, 0.0, half);
    let mut out = Vec::new();
    for (i, uv) in uvs.iter().enumerate().take(32) {
        if icon.mask >> i & 1 == 0 {
            continue;
        }
        let [u0, v0, u1, v1] = *uv;
        let corners = [
            (p - hr + hu, [u0, v0]),
            (p + hr + hu, [u1, v0]),
            (p + hr - hu, [u1, v1]),
            (p - hr - hu, [u0, v1]),
        ];
        for (c, uv) in [
            corners[0], corners[1], corners[2], corners[0], corners[2], corners[3],
        ] {
            out.push(IconVertex {
                position: (icon.anchor + c).to_array(),
                uv,
            });
        }
        p += up * size;
    }
    out
}

const SHADER: &str = r#"
struct Camera { view_proj: mat4x4<f32>, color: vec4<f32> };
@group(0) @binding(0) var<uniform> camera: Camera;
@group(1) @binding(0) var sheet: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;
struct Out { @builtin(position) clip: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex fn vs(@location(0) p: vec3<f32>, @location(1) uv: vec2<f32>) -> Out {
    var o: Out;
    o.clip = camera.view_proj * vec4<f32>(p, 1.0);
    o.uv = uv;
    return o;
}
@fragment fn fs(i: Out) -> @location(0) vec4<f32> {
    return textureSample(sheet, samp, i.uv) * camera.color;
}
"#;

pub struct StatusIconRenderer {
    pipeline: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
    camera: wgpu::BindGroup,
    texture_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    sheets: BTreeMap<String, wgpu::BindGroup>,
    /// Per sheet: vertex buffer and count for this frame.
    batches: Vec<(String, wgpu::Buffer, u32)>,
}

impl StatusIconRenderer {
    pub fn new(device: &wgpu::Device) -> Self {
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("status icons camera"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("status icons sheet"),
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
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("status icons camera"),
            size: 80,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let camera = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("status icons camera"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("status icons"),
            bind_group_layouts: &[Some(&camera_layout), Some(&texture_layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("status icons"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        const ATTRS: [wgpu::VertexAttribute; 2] =
            wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("status icons"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<IconVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &ATTRS,
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: crate::scene_gfx::SCENE_FORMAT,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            // Inferred: a translucent scene geometry (render class 2) tested against depth,
            // without writing it.
            depth_stencil: Some(wgpu::DepthStencilState {
                format: crate::scene_gfx::DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: crate::scene_gfx::multisample(),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("status icons"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            pipeline,
            uniform,
            camera,
            texture_layout,
            sampler,
            sheets: BTreeMap::new(),
            batches: Vec::new(),
        }
    }

    /// Build this frame's quads (read-only) and upload any new sheet textures.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        icons: &[StatusIcon],
        sheets: &BTreeMap<String, IconSheet>,
        frame: &crate::render::SceneFrame,
        options: IconOptions,
    ) {
        self.batches.clear();
        if !options.enabled {
            return;
        }
        let mut by_sheet: BTreeMap<&str, Vec<IconVertex>> = BTreeMap::new();
        for icon in icons.iter().filter(|i| i.mask != 0) {
            let Some(sheet) = sheets.get(&icon.sheet) else {
                continue;
            };
            by_sheet.entry(&icon.sheet).or_default().extend(quads(
                icon,
                &sheet.uvs,
                frame.view_proj,
                frame.right,
                frame.up,
                options,
            ));
        }
        for (name, vertices) in by_sheet {
            if vertices.is_empty() {
                continue;
            }
            if !self.sheets.contains_key(name) {
                let s = &sheets[name];
                let view = banished_gfx::upload_rgba(
                    device,
                    queue,
                    name,
                    s.width,
                    s.height,
                    &[s.rgba.clone()],
                );
                let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some(name),
                    layout: &self.texture_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&self.sampler),
                        },
                    ],
                });
                self.sheets.insert(name.to_string(), bind);
            }
            let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("status icons"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
            self.batches
                .push((name.to_string(), buffer, vertices.len() as u32));
        }
        let alpha = options.opacity as f32 / 100.0;
        let mut data = frame.view_proj.to_cols_array().to_vec();
        data.extend_from_slice(&[1.0, 1.0, 1.0, alpha]);
        queue.write_buffer(&self.uniform, 0, bytemuck::cast_slice(&data));
    }

    pub fn draw(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        depth: &wgpu::TextureView,
    ) {
        if self.batches.is_empty() {
            return;
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("status icons"),
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
        pass.set_bind_group(0, &self.camera, &[]);
        for (sheet, buffer, count) in &self.batches {
            let Some(bind) = self.sheets.get(sheet) else {
                continue;
            };
            pass.set_bind_group(1, bind, &[]);
            pass.set_vertex_buffer(0, buffer.slice(..));
            pass.draw(0..*count, 0..1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_quad_per_set_bit_stacked_up() {
        let icon = StatusIcon {
            anchor: Vec3::new(1.0, 2.0, 3.0),
            mask: 0b101,
            sheet: String::new(),
            size: 0.5,
            screen_constant: false,
        };
        let uvs = vec![[0.0, 0.0, 0.5, 0.5]; 3];
        let v = quads(
            &icon,
            &uvs,
            glam::Mat4::IDENTITY,
            Vec3::X,
            Vec3::Z,
            IconOptions::default(),
        );
        assert_eq!(v.len(), 12);
        // First quad centre 0.25 above the anchor, second 0.5 higher.
        let centre = |q: &[IconVertex]| (q[0].position[2] + q[2].position[2]) * 0.5;
        assert!((centre(&v[0..6]) - 3.25).abs() < 1e-6);
        assert!((centre(&v[6..12]) - 3.75).abs() < 1e-6);
    }
}
