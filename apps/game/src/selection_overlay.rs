//! The tools' cell overlay (`FUN_140105d90`): one terrain-hugging quad per cell, drawn with
//! `Terrain/Selection.rsc` (DebugTextureMaterial over `Terrain\Selection.png`, an 8-frame strip)
//! and a vertex colour. The removal tool (`FUN_140109520`) uses frame 3 in opaque red where its
//! drag acts and frame 0 in half-transparent green elsewhere. Building the vertices reads only
//! presentation state.

use wgpu::util::DeviceExt;

/// One cell of the overlay: map cell, strip frame (`+8`), ARGB colour (`+0xc`) and the
/// frame's rotation in degrees (`+0x10`; the placement tool's arrows, 0 for the drag tools).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OverlayCell {
    pub x: i32,
    pub y: i32,
    pub frame: i32,
    pub argb: u32,
    pub rotation: i32,
}

impl OverlayCell {
    /// `FUN_140109520`'s entry: `(-(acts) & 3, acts ? 0xffff0000 : 0x8000ff00)`.
    pub fn removal(x: i32, y: i32, acts: bool) -> OverlayCell {
        OverlayCell {
            x,
            y,
            frame: if acts { 3 } else { 0 },
            argb: if acts { 0xffff_0000 } else { 0x8000_ff00 },
            rotation: 0,
        }
    }
}

/// One line of the overlay (`FUN_140105d90`'s first list, 0x20 bytes): from cell `a` to cell
/// `b` (`+0`, `+0x14`), strip frame (`+8`), ARGB colour (`+0xc`), rotation (`+0x10`, 0 here)
/// and half its width (`+0x1c`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OverlayLine {
    pub a: (i32, i32),
    pub b: (i32, i32),
    pub frame: i32,
    pub argb: u32,
    pub width: f32,
}

/// The lines of `FUN_140105d90`, drawn before the cells: each a quad between the two cells'
/// centres on the ground, `width` either side; the frame's u runs across it, v along it.
pub fn line_vertices(
    lines: &[OverlayLine],
    map_size: i32,
    height: impl Fn(f32, f32) -> f32,
) -> Vec<OverlayVertex> {
    let half = (map_size >> 1) as f32 - 0.5;
    let mut out = Vec::with_capacity(lines.len() * 6);
    for l in lines {
        let point = |(x, y): (i32, i32)| {
            let (px, py) = (x as f32 - half, y as f32 - half);
            glam::Vec3::new(px, py, height(px, py) + LIFT)
        };
        let (p0, p1) = (point(l.a), point(l.b));
        let d = (p1 - p0).normalize_or_zero();
        let side = glam::Vec3::new(-d.y * l.width, d.x * l.width, 0.0);
        let [a, r, g, b] = l.argb.to_be_bytes();
        let corners = [
            (p0 + side, (0.0, 0.0)),
            (p1 - side, (1.0, 1.0)),
            (p0 - side, (1.0, 0.0)),
            (p0 + side, (0.0, 0.0)),
            (p1 + side, (0.0, 1.0)),
            (p1 - side, (1.0, 1.0)),
        ];
        for (p, (u, v)) in corners {
            out.push(OverlayVertex {
                position: p.to_array(),
                uv: [(u + l.frame as f32) * 0.125, v],
                color: [r, g, b, a],
            });
        }
    }
    out
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct OverlayVertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
    pub color: [u8; 4],
}

/// The overlay texture: RGBA pixels of `Terrain/Selection.rsc`'s diffuse image.
#[derive(Debug, Clone, PartialEq)]
pub struct OverlayTexture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub fn load_texture(archive: &banished_formats::pkg::Archive) -> Option<OverlayTexture> {
    let mi = banished_formats::types::material_instance(
        &archive.read_ref("Terrain/Selection.rsc:resource").ok()?,
    )
    .ok()?;
    let image = mi.texture("diffuse")?.replace('\\', "/");
    let img = banished_formats::types::image_buffer(&archive.read_ref(&image).ok()?).ok()?;
    let rgba = banished_formats::texture::decode_rgba(
        img.format,
        img.width,
        img.height,
        img.levels.first()?,
    )?;
    Some(OverlayTexture {
        width: img.width,
        height: img.height,
        rgba,
    })
}

/// Deviation: the quads sit this far above the terrain surface instead of relying on the
/// original material's depth state (not traced), so they do not fight the terrain's depth.
const LIFT: f32 = 0.02;
/// `DAT_1401787cc`: π / 180.
const DEGREES: f32 = 0.017_453_292;

/// `FUN_1400b65b0` and the cell loop of `FUN_140105d90`: two triangles per cell over the terrain
/// vertices, split along the terrain's own diagonal (by `x + y` parity). The cell's x and y
/// fractions turn about the cell's centre by its rotation (`DAT_1401787cc` degrees to radians:
/// `u' = (v − ½) sin + (u − ½) cos + ½`, `v' = (v − ½) cos − (u − ½) sin + ½`), and `u'` is
/// scaled into the strip frame (`DAT_1401787ec`, ⅛).
pub fn vertices(
    cells: &[OverlayCell],
    map_size: i32,
    height: impl Fn(f32, f32) -> f32,
) -> Vec<OverlayVertex> {
    // Even cells: (0,0) (0,1) (1,0), (1,0) (0,1) (1,1); odd cells: (0,0) (1,1) (1,0),
    // (0,0) (0,1) (1,1).
    const EVEN: [(f32, f32); 6] = [(0., 0.), (0., 1.), (1., 0.), (1., 0.), (0., 1.), (1., 1.)];
    const ODD: [(f32, f32); 6] = [(0., 0.), (1., 1.), (1., 0.), (0., 0.), (0., 1.), (1., 1.)];
    let half = (map_size >> 1) as f32;
    let mut out = Vec::with_capacity(cells.len() * 6);
    for c in cells {
        let corners = if (c.x + c.y) & 1 == 0 { &EVEN } else { &ODD };
        let [a, r, g, b] = c.argb.to_be_bytes();
        let (sin, cos) = (c.rotation as f32 * DEGREES).sin_cos();
        for &(u, v) in corners {
            let (px, py) = (c.x as f32 + u - half, c.y as f32 + v - half);
            let tu = (v - 0.5) * sin + (u - 0.5) * cos + 0.5;
            let tv = (v - 0.5) * cos - (u - 0.5) * sin + 0.5;
            out.push(OverlayVertex {
                position: [px, py, height(px, py) + LIFT],
                uv: [(tu + c.frame as f32) * 0.125, tv],
                color: [r, g, b, a],
            });
        }
    }
    out
}

const SHADER: &str = r#"
struct Camera { view_proj: mat4x4<f32> };
@group(0) @binding(0) var<uniform> camera: Camera;
@group(1) @binding(0) var strip: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;
struct Out {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};
@vertex fn vs(@location(0) p: vec3<f32>, @location(1) uv: vec2<f32>, @location(2) c: vec4<f32>) -> Out {
    var o: Out;
    o.clip = camera.view_proj * vec4<f32>(p, 1.0);
    o.uv = uv;
    o.color = c;
    return o;
}
// Debug.srsl Texture_ps (inferred): the texture modulated by the vertex colour.
@fragment fn fs(i: Out) -> @location(0) vec4<f32> {
    return textureSample(strip, samp, i.uv) * i.color;
}
"#;

pub struct OverlayRenderer {
    pipeline: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
    camera: wgpu::BindGroup,
    texture_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    texture: Option<wgpu::BindGroup>,
    batch: Option<(wgpu::Buffer, u32)>,
}

impl OverlayRenderer {
    pub fn new(device: &wgpu::Device) -> Self {
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cell overlay camera"),
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
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cell overlay texture"),
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
            label: Some("cell overlay camera"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let camera = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cell overlay camera"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("cell overlay"),
            bind_group_layouts: &[Some(&camera_layout), Some(&texture_layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cell overlay"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        const ATTRS: [wgpu::VertexAttribute; 3] =
            wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Unorm8x4];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("cell overlay"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<OverlayVertex>() as u64,
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
            // Inferred, like the status icons: tested against depth, not written.
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
            label: Some("cell overlay"),
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
            texture: None,
            batch: None,
        }
    }

    /// Upload this frame's quads (and the texture the first time it is available).
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        vertices: &[OverlayVertex],
        texture: Option<&OverlayTexture>,
        frame: &crate::render::SceneFrame,
    ) {
        self.batch = None;
        if vertices.is_empty() {
            return;
        }
        if self.texture.is_none() {
            let Some(t) = texture else { return };
            let view = banished_gfx::upload_rgba(
                device,
                queue,
                "Terrain/Selection",
                t.width,
                t.height,
                &[t.rgba.clone()],
            );
            self.texture = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("cell overlay texture"),
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
            }));
        }
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cell overlay"),
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        self.batch = Some((buffer, vertices.len() as u32));
        queue.write_buffer(
            &self.uniform,
            0,
            bytemuck::cast_slice(&frame.view_proj.to_cols_array()),
        );
    }

    pub fn draw(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        depth: &wgpu::TextureView,
    ) {
        let (Some((buffer, count)), Some(texture)) = (&self.batch, &self.texture) else {
            return;
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("cell overlay"),
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
        pass.set_bind_group(1, texture, &[]);
        pass.set_vertex_buffer(0, buffer.slice(..));
        pass.draw(0..*count, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_follow_the_terrain_diagonal() {
        let v = vertices(&[OverlayCell::removal(3, 4, true)], 8, |_, _| 1.0);
        assert_eq!(v.len(), 6);
        // Odd cell: the second vertex is the far corner; frame 3 of 8.
        assert_eq!(v[1].position, [0.0, 1.0, 1.0 + LIFT]);
        assert_eq!(v[1].uv, [0.5, 1.0]);
        assert_eq!(v[0].color, [0xff, 0, 0, 0xff]);
        let g = vertices(&[OverlayCell::removal(2, 4, false)], 8, |_, _| 0.0);
        assert_eq!(g[1].position[..2], [-2.0, 1.0]);
        assert_eq!(g[0].color, [0, 0xff, 0, 0x80]);
    }

    /// A rotated cell turns its frame about the centre: at 90° the corner `(0, 0)` samples
    /// the frame's `(0, 1)`, still inside frame 2.
    #[test]
    fn a_rotated_cell_turns_its_frame() {
        let cell = OverlayCell {
            x: 2,
            y: 4,
            frame: 2,
            argb: 0xff00_ff00,
            rotation: 90,
        };
        let v = vertices(&[cell], 8, |_, _| 0.0);
        let close =
            |a: [f32; 2], b: [f32; 2]| (a[0] - b[0]).abs() < 1e-5 && (a[1] - b[1]).abs() < 1e-5;
        assert!(close(v[0].uv, [0.25, 1.0]), "{:?}", v[0].uv);
        for x in &v {
            assert!(
                (0.25 - 1e-5..=0.375 + 1e-5).contains(&x.uv[0]),
                "{:?}",
                x.uv
            );
        }
    }
}
