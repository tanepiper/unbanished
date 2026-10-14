//! Placed static meshes (menu scenes now; buildings and resources in game later): CPU loading of
//! `GraphicsMesh` resources and an instanced wgpu renderer.
//!
//! Each placement is `FromYawPitchOffset(yaw, offset)` (`FUN_1400a9030`): a rotation about z by
//! `yaw` degrees and a translation. Every scene mesh uses rigid triangle lists (one node
//! transform per range), so no skinning is needed here. Shading is an approximation of the model
//! materials until their shaders are ported: diffuse with alpha test, sun + ambient (with the
//! part's AO texture) and the same fog as the terrain.

use std::collections::HashMap;

use banished_formats::menu_scene::MenuScene;
use banished_formats::pkg::Archive;
use banished_formats::skeleton::{local_bind, world_transforms, Transform};
use banished_formats::texture::decode_rgba;
use banished_formats::types::{
    graphics_mesh, image_buffer, material_instance, ImageBuffer, STATIC_STRIDE,
};
use glam::{Mat4, Quat, Vec3};
use wgpu::util::DeviceExt;

/// Which of the ported model shaders a range uses, from its material's base material.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MaterialKind {
    /// `Material\Opaque`, `OpaqueSnow`, `OpaqueInWater` (`OpaqueMaterial_ps`).
    Opaque,
    /// `Material\AlphaTest`, `Material\Foliage`: opaque with a 0.5 alpha cut.
    AlphaTest,
    /// `Material\Billboard`: leaf cards expanded towards the camera (`BillboardMaterial_vs/ps`).
    Billboard,
}

impl MaterialKind {
    fn of(base: Option<&str>) -> MaterialKind {
        let base = base.unwrap_or_default().replace('/', "\\");
        if base.contains("\\Billboard\\") {
            MaterialKind::Billboard
        } else if base.contains("\\AlphaTest\\") || base.contains("\\Foliage\\") {
            MaterialKind::AlphaTest
        } else {
            MaterialKind::Opaque
        }
    }
}

/// One draw range of a mesh with its material textures.
pub struct RangeData {
    pub kind: MaterialKind,
    pub vertices: Vec<u8>,
    pub indices: Vec<u8>,
    pub index_count: u32,
    pub diffuse: Option<ImageBuffer>,
    pub ao: Option<ImageBuffer>,
    pub rigid: [f32; 16],
}

/// A mesh (most detailed level) and the transforms of its instances.
pub struct ModelGroup {
    pub name: String,
    pub ranges: Vec<RangeData>,
    pub instances: Vec<[f32; 16]>,
}

pub struct SceneModels {
    pub groups: Vec<ModelGroup>,
}

fn with_object(reference: &str) -> String {
    let path = reference.replace('\\', "/");
    if path.contains(':') {
        path
    } else {
        format!("{path}:resource")
    }
}

fn image(archive: &Archive, reference: Option<&str>) -> Option<ImageBuffer> {
    let r = reference?;
    image_buffer(&archive.read_ref(&with_object(r)).ok()?).ok()
}

/// A placement's transform: `FromYawPitchOffset(yaw, offset)` (yaw in degrees about z) with the
/// per-axis scale the shaders read from the transform rows' `w` (placement floats 3..6).
pub fn placement_matrix(position: [f32; 3], scale: [f32; 3], yaw: f32) -> [f32; 16] {
    Mat4::from_scale_rotation_translation(
        Vec3::from(scale),
        Quat::from_rotation_z(yaw.to_radians()),
        Vec3::from(position),
    )
    .to_cols_array()
}

fn load_group(
    archive: &Archive,
    mesh_ref: &str,
    material_set: usize,
    textures: &mut HashMap<String, Option<ImageBuffer>>,
) -> Option<Vec<RangeData>> {
    let mesh = graphics_mesh(&archive.read_ref(&with_object(mesh_ref)).ok()?)
        .map_err(|e| log::warn!("{mesh_ref}: {e}"))
        .ok()?;
    let locals: Vec<Transform> = mesh.bones.iter().map(local_bind).collect();
    let world = world_transforms(&mesh.bones, &locals);
    let part = mesh.parts.first()?;
    let set = mesh
        .material_instances
        .get(material_set)
        .or_else(|| mesh.material_instances.first());
    let mut cached = |r: Option<String>| -> Option<ImageBuffer> {
        let r = r?;
        textures
            .entry(r.clone())
            .or_insert_with(|| image(archive, Some(&r)))
            .clone()
    };
    let ao = cached(part.ao_texture.clone());
    let mut ranges = Vec::new();
    for range in &part.ranges {
        if range.bone < 0 || range.index_count == 0 {
            continue;
        }
        let (index_data, vertex_data, _) = part.geometry(range);
        let first = range.start_index as usize * 2;
        let end = (range.start_index + range.index_count) as usize * 2;
        let v0 = range.start_vertex as usize * STATIC_STRIDE;
        let v1 = v0 + range.vertex_count as usize * STATIC_STRIDE;
        let (Some(indices), Some(vertices)) = (index_data.get(first..end), vertex_data.get(v0..v1))
        else {
            continue;
        };
        let mut indices = indices.to_vec();
        indices.resize(indices.len().next_multiple_of(4), 0);
        let instance = set
            .and_then(|s| s.get(range.material_index as usize).cloned().flatten())
            .and_then(|mi| material_instance(&archive.read_ref(&with_object(&mi)).ok()?).ok());
        let diffuse_ref = instance
            .as_ref()
            .and_then(|mi| mi.texture("diffuse").map(str::to_owned));
        let kind = MaterialKind::of(instance.as_ref().and_then(|mi| mi.material.as_deref()));
        ranges.push(RangeData {
            kind,
            vertices: vertices.to_vec(),
            indices,
            index_count: range.index_count as u32,
            diffuse: cached(diffuse_ref),
            ao: ao.clone(),
            rigid: world
                .get(range.bone as usize)
                .copied()
                .unwrap_or(Transform::IDENTITY)
                .to_matrix(),
        });
    }
    Some(ranges)
}

impl SceneModels {
    /// Group a menu scene's placements by mesh and material set (`FUN_1400a9030`).
    pub fn from_menu_scene(archive: &Archive, scene: &MenuScene) -> SceneModels {
        let mut by_key: HashMap<(i16, i16), Vec<[f32; 16]>> = HashMap::new();
        for p in &scene.placements {
            let f = p.floats;
            by_key
                .entry((p.mesh, p.material))
                .or_default()
                .push(placement_matrix(
                    [f[0], f[1], f[2]],
                    [f[3], f[4], f[5]],
                    f[6],
                ));
        }
        let mut textures = HashMap::new();
        let mut keys: Vec<_> = by_key.keys().copied().collect();
        keys.sort();
        let mut groups = Vec::new();
        for key in keys {
            let Some(Some(mesh_ref)) = scene.meshes.get(key.0 as usize) else {
                continue;
            };
            let Some(ranges) = load_group(archive, mesh_ref, key.1.max(0) as usize, &mut textures)
            else {
                continue;
            };
            groups.push(ModelGroup {
                name: mesh_ref.clone(),
                ranges,
                instances: by_key.remove(&key).unwrap_or_default(),
            });
        }
        SceneModels { groups }
    }
}

const SHADER: &str = r#"
struct Globals {
    view_proj: mat4x4<f32>,
    light_dir: vec4<f32>,
    light_color: vec4<f32>,
    ambient_color: vec4<f32>,
    fog_color: vec4<f32>,
    fog: vec4<f32>,
    user: vec4<f32>,
    camera_right: vec4<f32>,
    camera_forward: vec4<f32>,
    camera_up: vec4<f32>,
};
@group(0) @binding(0) var<uniform> g: Globals;
@group(1) @binding(0) var diffuse_tex: texture_2d<f32>;
@group(1) @binding(1) var ao_tex: texture_2d<f32>;
@group(1) @binding(2) var samp: sampler;
@group(1) @binding(3) var<uniform> rigid: mat4x4<f32>;

struct VsIn {
    @location(0) position: vec4<i32>,
    @location(1) texcoord: vec4<i32>,
    @location(2) normal: vec4<f32>,
    @location(3) corner: vec2<i32>,
    @location(5) m0: vec4<f32>,
    @location(6) m1: vec4<f32>,
    @location(7) m2: vec4<f32>,
    @location(8) m3: vec4<f32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) uv_ao: vec2<f32>,
    // x: diffuse (-n.L), y: ambient factor (n.L / 2 + 1.5), z: fog, w: leaf flag.
    @location(2) light: vec4<f32>,
};

fn finish(world: vec3<f32>, n: vec3<f32>, texcoord: vec4<i32>, leaf: f32) -> VsOut {
    var out: VsOut;
    out.clip = g.view_proj * vec4<f32>(world, 1.0);
    out.uv = vec2<f32>(texcoord.xy) / 512.0;
    out.uv_ao = vec2<f32>(texcoord.zw) / 8192.0;
    let d = dot(n, g.light_dir.xyz);
    out.light = vec4<f32>(-d, d * 0.5 + 1.5, (out.clip.w + g.fog.x) * g.fog.y, leaf);
    return out;
}

@vertex
fn vs_main(v: VsIn) -> VsOut {
    let model = mat4x4<f32>(v.m0, v.m1, v.m2, v.m3) * rigid;
    let p = model * vec4<f32>(vec3<f32>(v.position.xyz) / 512.0, 1.0);
    let n = normalize((model * vec4<f32>(v.normal.xyz * 2.0 - 1.0, 0.0)).xyz);
    return finish(p.xyz, n, v.texcoord, 0.0);
}

// BillboardMaterial_vs: vertices with a corner offset (userData) are leaf cards. They are pushed
// out along the camera's right and up axes (bent towards the view by the normal), sway with the
// wind, and are lit with a rounded crown normal.
@vertex
fn vs_billboard(v: VsIn) -> VsOut {
    let model = mat4x4<f32>(v.m0, v.m1, v.m2, v.m3) * rigid;
    let local = vec3<f32>(v.position.xyz) / 512.0;
    var world = (model * vec4<f32>(local, 1.0)).xyz;
    let scale = vec3<f32>(length(v.m0.xyz), length(v.m1.xyz), length(v.m2.xyz));
    let n = normalize((model * vec4<f32>(normalize(v.normal.xyz * 2.0 - 1.0), 0.0)).xyz);
    let corner = vec2<f32>(v.corner);
    let leaf = select(0.0, 1.0, dot(corner, corner) >= 0.05);
    let right = g.camera_right.xyz;
    let forward = g.camera_forward.xyz;
    let up = g.camera_up.xyz;
    let nc = vec3<f32>(dot(n, right), dot(n, forward), dot(n, up)) * leaf;
    let r = normalize(right + 0.5 * nc.x * forward) * scale.x;
    let u = normalize(up + 0.5 * nc.z * forward) * scale.z;
    let wind = sin(world.xy + g.camera_forward.w) * 0.025 * dot(nc, nc) * scale.xz * leaf;
    world += r * wind.x + u * wind.y;
    let c = corner / 8192.0;
    world += r * c.x + u * c.y;
    let crown = normalize(vec3<f32>(local.x * scale.x, local.y * scale.y, local.z * scale.z + 4.0));
    let lit_normal = mix(n, crown, leaf);
    return finish(world, lit_normal, v.texcoord, leaf);
}

fn shade(base: vec3<f32>, in: VsOut) -> vec4<f32> {
    let ao = textureSample(ao_tex, samp, in.uv_ao).r;
    let light = g.ambient_color.rgb * in.light.y * ao + g.light_color.rgb * clamp(in.light.x, 0.0, 1.0);
    let lit = base * light;
    let fog = clamp(min(g.fog_color.w, in.light.z), 0.0, 1.0);
    return vec4<f32>(lit + fog * (g.fog_color.rgb - lit), 1.0);
}

@fragment
fn fs_opaque(in: VsOut) -> @location(0) vec4<f32> {
    return shade(textureSample(diffuse_tex, samp, in.uv).rgb, in);
}

@fragment
fn fs_alpha(in: VsOut) -> @location(0) vec4<f32> {
    let base = textureSample(diffuse_tex, samp, in.uv);
    if (base.a < 0.5) {
        discard;
    }
    return shade(base.rgb, in);
}

// BillboardMaterial_ps: leaves thin out with userPosition.z and turn autumn-coloured with .y.
@fragment
fn fs_billboard(in: VsOut) -> @location(0) vec4<f32> {
    var base = textureSample(diffuse_tex, samp, in.uv);
    let alpha = base.a - in.light.w * g.user.z;
    if (alpha < 0.5) {
        discard;
    }
    let lum = dot(base.rgb, base.rgb);
    let autumn = lum * vec3<f32>(0.913725495, 0.725490212, 0.278431386);
    let rgb = base.rgb + in.light.w * g.user.y * (autumn - base.rgb);
    return shade(rgb, in);
}
"#;

struct GpuRange {
    kind: MaterialKind,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    index_count: u32,
    bind: wgpu::BindGroup,
}

struct GpuGroup {
    ranges: Vec<GpuRange>,
    instances: wgpu::Buffer,
    count: u32,
}

pub struct ModelRenderer {
    pipelines: [wgpu::RenderPipeline; 3],
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    white: wgpu::TextureView,
    groups: Vec<GpuGroup>,
}

impl ModelRenderer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        globals_layout: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
        depth: wgpu::TextureFormat,
    ) -> ModelRenderer {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("models"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
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
            label: Some("model material"),
            entries: &[
                tex(0),
                tex(1),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
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
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("models"),
            bind_group_layouts: &[Some(globals_layout), Some(&layout)],
            immediate_size: 0,
        });
        let make = |vs: &str, fs: &str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(fs),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vs),
                    compilation_options: Default::default(),
                    buffers: &[
                        Some(wgpu::VertexBufferLayout {
                            array_stride: STATIC_STRIDE as u64,
                            step_mode: wgpu::VertexStepMode::Vertex,
                            attributes: &wgpu::vertex_attr_array![
                                0 => Sint16x4, 1 => Sint16x4, 2 => Unorm8x4, 3 => Sint16x2
                            ],
                        }),
                        Some(wgpu::VertexBufferLayout {
                            array_stride: 64,
                            step_mode: wgpu::VertexStepMode::Instance,
                            attributes: &wgpu::vertex_attr_array![
                                5 => Float32x4, 6 => Float32x4, 7 => Float32x4, 8 => Float32x4
                            ],
                        }),
                    ],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::COLOR,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    // Foliage cards are single-sided in the data but seen from both sides.
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: depth,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::Less),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let pipelines = [
            make("vs_main", "fs_opaque"),
            make("vs_main", "fs_alpha"),
            make("vs_billboard", "fs_billboard"),
        ];
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            anisotropy_clamp: 8,
            ..Default::default()
        });
        let white = banished_gfx::upload_rgba_as(
            device,
            queue,
            "white",
            1,
            1,
            &[vec![255; 4]],
            wgpu::TextureFormat::Rgba8Unorm,
        );
        ModelRenderer {
            pipelines,
            layout,
            sampler,
            white,
            groups: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        self.groups.clear();
    }

    pub fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, models: &SceneModels) {
        let mut views: HashMap<usize, wgpu::TextureView> = HashMap::new();
        let mut view_of = |img: &Option<ImageBuffer>| -> wgpu::TextureView {
            let Some(img) = img else {
                return self.white.clone();
            };
            // Images are shared between ranges; key by their data address.
            let key = img.levels.first().map_or(0, |l| l.as_ptr() as usize);
            views
                .entry(key)
                .or_insert_with(|| {
                    let mips: Vec<Vec<u8>> = (0..img.mip_count().max(1))
                        .map_while(|i| {
                            decode_rgba(
                                img.format,
                                (img.width >> i).max(1),
                                (img.height >> i).max(1),
                                img.levels.get(i)?,
                            )
                        })
                        .collect();
                    if mips.is_empty() {
                        return self.white.clone();
                    }
                    banished_gfx::upload_rgba_as(
                        device,
                        queue,
                        "model texture",
                        img.width,
                        img.height,
                        &mips,
                        wgpu::TextureFormat::Rgba8Unorm,
                    )
                })
                .clone()
        };
        self.groups = models
            .groups
            .iter()
            .map(|g| {
                let ranges = g
                    .ranges
                    .iter()
                    .map(|r| {
                        let rigid = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("rigid"),
                            contents: bytemuck::cast_slice(&r.rigid),
                            usage: wgpu::BufferUsages::UNIFORM,
                        });
                        let (diffuse, ao) = (view_of(&r.diffuse), view_of(&r.ao));
                        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                            label: Some(&g.name),
                            layout: &self.layout,
                            entries: &[
                                wgpu::BindGroupEntry {
                                    binding: 0,
                                    resource: wgpu::BindingResource::TextureView(&diffuse),
                                },
                                wgpu::BindGroupEntry {
                                    binding: 1,
                                    resource: wgpu::BindingResource::TextureView(&ao),
                                },
                                wgpu::BindGroupEntry {
                                    binding: 2,
                                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                                },
                                wgpu::BindGroupEntry {
                                    binding: 3,
                                    resource: rigid.as_entire_binding(),
                                },
                            ],
                        });
                        GpuRange {
                            kind: r.kind,
                            vertices: device.create_buffer_init(
                                &wgpu::util::BufferInitDescriptor {
                                    label: Some(&g.name),
                                    contents: &r.vertices,
                                    usage: wgpu::BufferUsages::VERTEX,
                                },
                            ),
                            indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: Some(&g.name),
                                contents: &r.indices,
                                usage: wgpu::BufferUsages::INDEX,
                            }),
                            index_count: r.index_count,
                            bind,
                        }
                    })
                    .collect();
                GpuGroup {
                    ranges,
                    instances: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("instances"),
                        contents: bytemuck::cast_slice(&g.instances),
                        usage: wgpu::BufferUsages::VERTEX,
                    }),
                    count: g.instances.len() as u32,
                }
            })
            .collect();
    }

    pub fn draw<'p>(&'p self, pass: &mut wgpu::RenderPass<'p>, globals: &'p wgpu::BindGroup) {
        if self.groups.is_empty() {
            return;
        }
        pass.set_bind_group(0, globals, &[]);
        for (i, kind) in [
            MaterialKind::Opaque,
            MaterialKind::AlphaTest,
            MaterialKind::Billboard,
        ]
        .into_iter()
        .enumerate()
        {
            pass.set_pipeline(&self.pipelines[i]);
            for g in &self.groups {
                pass.set_vertex_buffer(1, g.instances.slice(..));
                for r in g.ranges.iter().filter(|r| r.kind == kind) {
                    pass.set_bind_group(1, &r.bind, &[]);
                    pass.set_vertex_buffer(0, r.vertices.slice(..));
                    pass.set_index_buffer(r.indices.slice(..), wgpu::IndexFormat::Uint16);
                    pass.draw_indexed(0..r.index_count, 0, 0..g.count);
                }
            }
        }
    }
}
