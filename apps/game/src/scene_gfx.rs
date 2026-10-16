//! GPU renderers for the sky, the water surface and the terrain decals, rewritten from the
//! disassembled D3D9 programs (`SkyMaterial`, `RiverMaterial` with its lake vertex shader,
//! `TerrainTiledSnowDecal`, `TerrainDecalBlend`, `TerrainDecal`). See docs/MENU-SCENE.md.

use banished_formats::types::ImageBuffer;
use wgpu::util::DeviceExt;

use crate::scene_assets::{DecalData, DecalKind, SceneExtras, SkyData, WaterData};

/// Shared uniform block, prepended to every scene shader (matches `render::Globals`).
pub const GLOBALS_WGSL: &str = r#"
struct Globals {
    view_proj: mat4x4<f32>,
    // Direction the sunlight travels (lc_nearPlane).
    light_dir: vec4<f32>,
    light_color: vec4<f32>,
    ambient_color: vec4<f32>,
    // rgb, w = maximum fog amount.
    fog_color: vec4<f32>,
    // x = -fog start, y = 1 / (fog end - fog start).
    fog: vec4<f32>,
    // pc_userPosition: x snow, y autumn, z leaf loss, w rain.
    user: vec4<f32>,
    camera_right: vec4<f32>,
    // w = time in seconds (gc_time).
    camera_forward: vec4<f32>,
    camera_up: vec4<f32>,
    // gc_cameraPosition.
    camera_pos: vec4<f32>,
    // Planar reflection pass: x = 1 when active, y = water height (geometry below is clipped).
    clip: vec4<f32>,
    // lc[10..13]: world -> sun shadow map (u, v, depth); see shadow.rs.
    shadow: mat4x4<f32>,
    // x = 1 / shadow map size (pc[0].x), y = 1 when the sun casts shadows.
    shadow_params: vec4<f32>,
};
@group(0) @binding(0) var<uniform> g: Globals;
@group(0) @binding(1) var shadow_map: texture_depth_2d;
@group(0) @binding(2) var shadow_cmp: sampler_comparison;

fn reflect_clip(world_z: f32) {
    if (g.clip.x > 0.5 && world_z < g.clip.y) {
        discard;
    }
}

fn shadow_coord(world: vec3<f32>) -> vec3<f32> {
    return (g.shadow * vec4<f32>(world, 1.0)).xyz;
}

// The e_shadowMap term of the lit pixel shaders at the default shadow detail (Medium, program
// 2): four hardware-compared taps at (±½, ±½) texels, averaged. It scales only the sun term.
// The reference depth is clamped to 0..1 as for the original's D24 shadow map.
fn sun_shadow(t: vec3<f32>) -> f32 {
    if (g.shadow_params.y < 0.5) {
        return 1.0;
    }
    let z = clamp(t.z, 0.0, 1.0);
    // ShaderDetail selects the program variant: 1 one tap, 2 four taps at ±½ texel, 3 nine
    // taps at ±1 texel.
    if (g.shadow_params.z < 1.5) {
        return textureSampleCompareLevel(shadow_map, shadow_cmp, t.xy, z);
    }
    if (g.shadow_params.z > 2.5) {
        let s = g.shadow_params.x;
        var sum = 0.0;
        for (var i = -1; i <= 1; i++) {
            for (var j = -1; j <= 1; j++) {
                let o = vec2<f32>(f32(i), f32(j)) * s;
                sum += textureSampleCompareLevel(shadow_map, shadow_cmp, t.xy + o, z);
            }
        }
        return sum / 9.0;
    }
    let h = 0.5 * g.shadow_params.x;
    let a = textureSampleCompareLevel(shadow_map, shadow_cmp, t.xy + vec2<f32>(h, h), z);
    let b = textureSampleCompareLevel(shadow_map, shadow_cmp, t.xy + vec2<f32>(h, -h), z);
    let c = textureSampleCompareLevel(shadow_map, shadow_cmp, t.xy + vec2<f32>(-h, h), z);
    let d = textureSampleCompareLevel(shadow_map, shadow_cmp, t.xy + vec2<f32>(-h, -h), z);
    return (a + b + c + d) * 0.25;
}
"#;

/// Colour format of the offscreen scene and reflection targets.
pub const SCENE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24PlusStencil8;

fn tex_entry(binding: u32, dim: wgpu::TextureViewDimension) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: dim,
            multisampled: false,
        },
        count: None,
    }
}

fn sampler_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }
}

pub fn sampler(device: &wgpu::Device, mode: wgpu::AddressMode) -> wgpu::Sampler {
    device.create_sampler(&material_sampler(mode))
}

/// `Video::Configuration` "TextureFilter" (+0x28) for samplers made from now on: −1 bilinear,
/// 0 trilinear, n ≥ 1 anisotropic n×. The renderer is rebuilt when it changes.
static TEXTURE_FILTER: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

pub fn set_texture_filter(filter: i32) {
    TEXTURE_FILTER.store(filter, std::sync::atomic::Ordering::Relaxed);
}

/// A material sampler with the configured filtering.
pub fn material_sampler(mode: wgpu::AddressMode) -> wgpu::SamplerDescriptor<'static> {
    let filter = TEXTURE_FILTER.load(std::sync::atomic::Ordering::Relaxed);
    wgpu::SamplerDescriptor {
        address_mode_u: mode,
        address_mode_v: mode,
        address_mode_w: mode,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: if filter < 0 {
            wgpu::MipmapFilterMode::Nearest
        } else {
            wgpu::MipmapFilterMode::Linear
        },
        anisotropy_clamp: filter.clamp(1, 16) as u16,
        ..Default::default()
    }
}

struct PipelineSpec<'a> {
    label: &'a str,
    vs: &'a str,
    fs: &'a str,
    stride: u64,
    attributes: &'a [wgpu::VertexAttribute],
    blend: Option<wgpu::BlendState>,
    depth_write: bool,
    depth_compare: wgpu::CompareFunction,
    bias: wgpu::DepthBiasState,
}

/// The scene's sample count (antialiasing, `Video::Configuration +0x24`), set when the
/// renderer starts: every pipeline drawing into the scene or the reflection is made with it.
static SAMPLES: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

pub fn set_samples(n: u32) {
    SAMPLES.store(n.max(1), std::sync::atomic::Ordering::Relaxed);
}

pub fn samples() -> u32 {
    SAMPLES.load(std::sync::atomic::Ordering::Relaxed)
}

/// The scene pipelines' multisample state.
pub fn multisample() -> wgpu::MultisampleState {
    wgpu::MultisampleState {
        count: samples(),
        ..Default::default()
    }
}

fn pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    spec: PipelineSpec,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(spec.label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(spec.vs),
            compilation_options: Default::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: spec.stride,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: spec.attributes,
            })],
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(spec.fs),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: SCENE_FORMAT,
                blend: spec.blend,
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
            depth_write_enabled: Some(spec.depth_write),
            depth_compare: Some(spec.depth_compare),
            stencil: Default::default(),
            bias: spec.bias,
        }),
        multisample: multisample(),
        multiview_mask: None,
        cache: None,
    })
}

fn linear_view(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    img: &Option<ImageBuffer>,
    fallback: &wgpu::TextureView,
) -> wgpu::TextureView {
    img.as_ref()
        .and_then(|i| banished_gfx::upload_image_linear(device, queue, "scene texture", i))
        .unwrap_or_else(|| fallback.clone())
}

// ------------------------------------------------------------------------------------- sky

const SKY_WGSL: &str = r#"
@group(1) @binding(0) var clear_cube: texture_cube<f32>;
@group(1) @binding(1) var samp: sampler;
@group(1) @binding(2) var<uniform> model: mat4x4<f32>;

struct SkyOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) dir: vec3<f32>,
};

@vertex
fn vs_sky(@location(0) position: vec4<i32>) -> SkyOut {
    var out: SkyOut;
    let local = vec3<f32>(position.xyz) * 0.001953125;
    out.dir = local;
    let world = model * vec4<f32>(local, 1.0);
    out.clip = g.view_proj * world;
    // Always on the far plane.
    out.clip.z = out.clip.w * 0.99999;
    return out;
}

// SkyMaterial_ps: clear sky from the upper half of the cube, overcast (z mirrored) mixed in by
// snow and rain.
@fragment
fn fs_sky(in: SkyOut) -> @location(0) vec4<f32> {
    let clear = textureSample(clear_cube, samp, in.dir);
    let overcast = textureSample(clear_cube, samp, in.dir * vec3<f32>(1.0, 1.0, -1.0));
    let s = g.user.x * 0.75;
    let t = g.user.w * (1.0 - s) + s;
    return vec4<f32>(mix(clear, overcast, t).rgb, 0.0);
}
"#;

struct SkyGpu {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    index_count: u32,
    model: wgpu::Buffer,
    bind: wgpu::BindGroup,
}

// ------------------------------------------------------------------------------------- water

const WATER_WGSL: &str = r#"
@group(1) @binding(0) var water_tex: texture_2d<f32>;
@group(1) @binding(1) var frame_tex: texture_2d<f32>;
@group(1) @binding(2) var reflection_tex: texture_2d<f32>;
@group(1) @binding(3) var highlight: texture_cube<f32>;
@group(1) @binding(4) var wrap: sampler;
@group(1) @binding(5) var clamp_s: sampler;

struct WaterIn {
    @location(0) pos: vec4<i32>,
    @location(1) uv: vec2<i32>,
    @location(2) normal: vec4<f32>,
};

struct WaterOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) t0: vec4<f32>,
    @location(1) view: vec3<f32>,
    @location(2) screen: vec4<f32>,
    // x: -L.z, y: L.z/2 + 1.5, z: fog, w: alpha.
    @location(3) t3: vec4<f32>,
    // Sun shadow map coordinate (t5).
    @location(4) sh: vec3<f32>,
};

fn water_common(v: WaterIn) -> WaterOut {
    var o: WaterOut;
    let world = vec3<f32>(f32(v.pos.x) * 0.020833334, f32(v.pos.y) * 0.020833334,
                          f32(v.pos.z) * 0.00390625);
    o.view = world - g.camera_pos.xyz;
    let clip = g.view_proj * vec4<f32>(world, 1.0);
    o.clip = clip;
    o.screen = vec4<f32>(0.5 * clip.x + 0.5 * clip.w, -0.5 * clip.y + 0.5 * clip.w, clip.z, clip.w);
    o.t3 = vec4<f32>(-g.light_dir.z, 0.5 * g.light_dir.z + 1.5,
                     (clip.w + g.fog.x) * g.fog.y, v.normal.z);
    o.sh = shadow_coord(world);
    return o;
}

// RiverMaterial_vs: both normal-map layers flow along v.
@vertex
fn vs_river(v: WaterIn) -> WaterOut {
    var o = water_common(v);
    let uv = vec2<f32>(v.uv) * 0.0009765625;
    let t = g.camera_forward.w;
    o.t0 = vec4<f32>(uv.x, uv.y + t * 0.02, uv.x, uv.y + t * 0.01);
    return o;
}

// RiverMaterial_Normal_lakevs: lakes drift slowly in two directions.
@vertex
fn vs_lake(v: WaterIn) -> WaterOut {
    var o = water_common(v);
    let uv = vec2<f32>(v.uv) * 0.0009765625;
    let t = g.camera_forward.w;
    o.t0 = vec4<f32>(uv.x, uv.y - 0.0057 * t, uv.x + 0.005 * t, uv.y + 0.01 * t);
    return o;
}

// RiverMaterial_ps.
@fragment
fn fs_water(i: WaterOut) -> @location(0) vec4<f32> {
    let a = textureSample(water_tex, wrap, i.t0.xy);
    let b = textureSample(water_tex, wrap, i.t0.zw);
    let d = vec2<f32>(a.w * 2.0 - 1.0 + b.w * 2.0 - 1.0, a.y * 2.0 - 1.0 + b.y * 2.0 - 1.0);
    let h = d * 0.5;
    let n = vec3<f32>(h, 1.0 - length(h));
    let v = normalize(i.view);
    let r = v - 2.0 * dot(v, n) * n;
    let screen = i.screen.xy / i.screen.w;
    let refl_uv = d * 0.3 + screen;
    let frame_uv = d * 0.025 + screen;
    let spec = textureSample(highlight, wrap, r).rgb * 1.5;
    let frame = textureSample(frame_tex, clamp_s, frame_uv);
    let refl = textureSample(reflection_tex, clamp_s, refl_uv);
    let nn = vec3<f32>(d * 2.0, n.z);
    let f = dot(-v, nn);
    let reflected = spec * refl.w + refl.rgb;
    let f2 = f * f;
    // Program 2 sums its four shadow taps against a quarter of program 0's constant.
    let sun = vec3<f32>(0.165, 0.1815, 0.165) * sun_shadow(i.sh);
    let colour = reflected * ((1.0 - f2) * 0.67) + (frame.rgb * (f2 * 0.67) + sun);
    let fog = min(g.fog_color.w, clamp(i.t3.z, 0.0, 1.0));
    let fogged = mix(colour, g.fog_color.rgb, fog);
    // The terrain wrote its underwater factor to alpha: shallow shores show the ground.
    return vec4<f32>(mix(frame.rgb, fogged, frame.w), i.t3.w);
}
"#;

struct WaterGpu {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    rivers: Vec<[i32; 4]>,
    lakes: Vec<[i32; 4]>,
    river_tex: wgpu::TextureView,
    lake_tex: wgpu::TextureView,
    highlight: wgpu::TextureView,
    river_bind: Option<wgpu::BindGroup>,
    lake_bind: Option<wgpu::BindGroup>,
    pub level: f32,
}

// ------------------------------------------------------------------------------------- decals

const DECAL_WGSL: &str = r#"
@group(1) @binding(0) var diffuse: texture_2d<f32>;
@group(1) @binding(1) var mask: texture_2d<f32>;
@group(1) @binding(2) var snow_tex: texture_2d<f32>;
@group(1) @binding(3) var samp: sampler;

struct DecalIn {
    @location(0) pos: vec4<i32>,
    @location(1) uv: vec4<i32>,
    @location(2) normal: vec4<f32>,
};

struct DecalOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) t0: vec4<f32>,
    // x diffuse, y ambient factor, z fog, w vertex alpha.
    @location(1) t1: vec4<f32>,
    // xyz world (Blend triplanar), w underwater factor.
    @location(2) t2: vec4<f32>,
    // Blend weights |n|^4.
    @location(3) t3: vec3<f32>,
    // Sun shadow map coordinate.
    @location(4) sh: vec3<f32>,
};

@vertex
fn vs_decal(v: DecalIn) -> DecalOut {
    var o: DecalOut;
    let n = normalize(v.normal.xyz * 2.0 - 1.0);
    let d = dot(n, g.light_dir.xyz);
    let world = vec3<f32>(f32(v.pos.x), f32(v.pos.y), f32(v.pos.z) * 0.00390625);
    o.clip = g.view_proj * vec4<f32>(world, 1.0);
    o.t1 = vec4<f32>(-d, (d + 3.0) * 0.5, (o.clip.w + g.fog.x) * g.fog.y,
                     f32(v.pos.w) * 0.00392156886);
    o.t0 = vec4<f32>(vec2<f32>(v.uv.xy), vec2<f32>(v.uv.zw));
    o.t2 = vec4<f32>(world.x * 0.0625, world.y * 0.0625, f32(v.pos.z) * 0.000244140625,
                     (f32(v.pos.z) * 0.00390625 + 0.75) * -0.666666687);
    let n2 = n * n;
    o.t3 = n2 * n2;
    o.sh = shadow_coord(world);
    return o;
}

fn light(base: vec3<f32>, t1: vec4<f32>, sh: vec3<f32>) -> vec3<f32> {
    let l = g.ambient_color.rgb * t1.y
        + g.light_color.rgb * sun_shadow(sh) * clamp(t1.x, 0.0, 1.0);
    let lit = base * l;
    let fog = min(g.fog_color.w, clamp(t1.z, 0.0, 1.0));
    return lit + fog * (g.fog_color.rgb - lit);
}

// TerrainTiledSnowDecalMaterial_ps: diffuse at uv/16 (its alpha marks where snow settles), the
// tile mask's green at zw/4096 gives coverage; darkened under water.
@fragment
fn fs_tiled(i: DecalOut) -> @location(0) vec4<f32> {
    let uv = i.t0.xy * 0.0625;
    let base = textureSample(diffuse, samp, uv);
    let s = textureSample(snow_tex, samp, uv);
    let c = mix(base.rgb, s.rgb, base.w * g.user.x);
    let lit = light(c, i.t1, i.sh);
    let under = clamp(i.t2.w, 0.0, 1.0);
    let m = textureSample(mask, samp, i.t0.zw * 0.000244140625);
    return vec4<f32>(lit * (1.0 - under), m.y * i.t1.w);
}

// TerrainDecalBlendMaterial_ps: the footprint texture over a triplanar ground texture.
@fragment
fn fs_blend(i: DecalOut) -> @location(0) vec4<f32> {
    let uv = i.t0.xy * 0.000244140625;
    let base = textureSample(diffuse, samp, uv);
    let s = textureSample(snow_tex, samp, uv);
    let px = textureSample(mask, samp, i.t2.yz);
    let py = textureSample(mask, samp, i.t2.xz);
    let pz = textureSample(mask, samp, uv.yx);
    let ground = px.rgb * i.t3.x + py.rgb * i.t3.y + pz.rgb * i.t3.z;
    let c0 = mix(ground, base.rgb, base.w);
    let c = mix(c0, s.rgb, i.t3.z * g.user.x);
    return vec4<f32>(light(c, i.t1, i.sh), pz.w);
}

// TerrainDecalMaterial_ps.
@fragment
fn fs_plain(i: DecalOut) -> @location(0) vec4<f32> {
    let uv = i.t0.xy * 0.000244140625;
    let base = textureSample(diffuse, samp, uv);
    let lit = light(base.rgb, i.t1, i.sh);
    let under = clamp(i.t2.w, 0.0, 1.0);
    return vec4<f32>(mix(lit, vec3<f32>(0.0250000004, 0.123000003, 0.189999998), under),
                     base.w * i.t1.w);
}
"#;

struct DecalGpu {
    kind: DecalKind,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    index_count: u32,
    bind: wgpu::BindGroup,
}

// ------------------------------------------------------------------------------------- renderer

pub struct SceneExtrasRenderer {
    sky_pipeline: wgpu::RenderPipeline,
    sky_layout: wgpu::BindGroupLayout,
    river_pipeline: wgpu::RenderPipeline,
    lake_pipeline: wgpu::RenderPipeline,
    water_layout: wgpu::BindGroupLayout,
    decal_pipelines: [wgpu::RenderPipeline; 3],
    decal_layout: wgpu::BindGroupLayout,
    wrap: wgpu::Sampler,
    clamp: wgpu::Sampler,
    white: wgpu::TextureView,
    sky: Option<SkyGpu>,
    water: Option<WaterGpu>,
    decals: Vec<DecalGpu>,
}

impl SceneExtrasRenderer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        globals_layout: &wgpu::BindGroupLayout,
    ) -> SceneExtrasRenderer {
        use wgpu::TextureViewDimension as D;
        let module = |label: &str, src: &str| {
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(label),
                source: wgpu::ShaderSource::Wgsl(format!("{GLOBALS_WGSL}{src}").into()),
            })
        };
        let layout = |label: &str, entries: &[wgpu::BindGroupLayoutEntry]| {
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some(label),
                entries,
            })
        };
        let pipeline_layout = |l: &wgpu::BindGroupLayout| {
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[Some(globals_layout), Some(l)],
                immediate_size: 0,
            })
        };

        let sky_layout = layout(
            "sky",
            &[
                tex_entry(0, D::Cube),
                sampler_entry(1),
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
        );
        let sky_shader = module("sky", SKY_WGSL);
        let sky_pipeline = pipeline(
            device,
            &sky_shader,
            &pipeline_layout(&sky_layout),
            PipelineSpec {
                label: "sky",
                vs: "vs_sky",
                fs: "fs_sky",
                stride: 24,
                attributes: &wgpu::vertex_attr_array![0 => Sint16x4],
                blend: None,
                depth_write: false,
                depth_compare: wgpu::CompareFunction::LessEqual,
                bias: Default::default(),
            },
        );

        let water_layout = layout(
            "water",
            &[
                tex_entry(0, D::D2),
                tex_entry(1, D::D2),
                tex_entry(2, D::D2),
                tex_entry(3, D::Cube),
                sampler_entry(4),
                sampler_entry(5),
            ],
        );
        let water_shader = module("water", WATER_WGSL);
        let water_attrs = wgpu::vertex_attr_array![0 => Sint16x4, 1 => Sint16x2, 2 => Unorm8x4];
        let water_pipeline = |vs: &'static str| {
            pipeline(
                device,
                &water_shader,
                &pipeline_layout(&water_layout),
                PipelineSpec {
                    label: vs,
                    vs,
                    fs: "fs_water",
                    stride: 16,
                    attributes: &water_attrs,
                    blend: None,
                    depth_write: true,
                    depth_compare: wgpu::CompareFunction::LessEqual,
                    bias: Default::default(),
                },
            )
        };
        let river_pipeline = water_pipeline("vs_river");
        let lake_pipeline = water_pipeline("vs_lake");

        let decal_layout = layout(
            "decal",
            &[
                tex_entry(0, D::D2),
                tex_entry(1, D::D2),
                tex_entry(2, D::D2),
                sampler_entry(3),
            ],
        );
        let decal_shader = module("decals", DECAL_WGSL);
        let decal_attrs = wgpu::vertex_attr_array![0 => Sint16x4, 1 => Sint16x4, 2 => Unorm8x4];
        // Blend colour by the decal's alpha but keep the terrain's alpha (its underwater factor,
        // which the water shader reads back).
        let decal_blend = wgpu::BlendState {
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
        let decal_pipeline = |fs: &'static str| {
            pipeline(
                device,
                &decal_shader,
                &pipeline_layout(&decal_layout),
                PipelineSpec {
                    label: fs,
                    vs: "vs_decal",
                    fs,
                    stride: 20,
                    attributes: &decal_attrs,
                    blend: Some(decal_blend),
                    depth_write: false,
                    depth_compare: wgpu::CompareFunction::LessEqual,
                    bias: wgpu::DepthBiasState {
                        constant: -4,
                        slope_scale: -1.0,
                        clamp: 0.0,
                    },
                },
            )
        };
        let decal_pipelines = [
            decal_pipeline("fs_tiled"),
            decal_pipeline("fs_blend"),
            decal_pipeline("fs_plain"),
        ];
        let white = banished_gfx::upload_rgba_as(
            device,
            queue,
            "white",
            1,
            1,
            &[vec![255; 4]],
            wgpu::TextureFormat::Rgba8Unorm,
        );
        SceneExtrasRenderer {
            sky_pipeline,
            sky_layout,
            river_pipeline,
            lake_pipeline,
            water_layout,
            decal_pipelines,
            decal_layout,
            wrap: sampler(device, wgpu::AddressMode::Repeat),
            clamp: sampler(device, wgpu::AddressMode::ClampToEdge),
            white,
            sky: None,
            water: None,
            decals: Vec::new(),
        }
    }

    pub fn water_level(&self) -> Option<f32> {
        self.water.as_ref().map(|w| w.level)
    }

    pub fn clear(&mut self) {
        self.sky = None;
        self.water = None;
        self.decals.clear();
    }

    pub fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, extras: &SceneExtras) {
        self.sky = extras
            .sky
            .as_ref()
            .and_then(|s| self.upload_sky(device, queue, s));
        self.water = extras
            .water
            .as_ref()
            .map(|w| self.upload_water(device, queue, w));
        self.upload_decals(device, queue, &extras.decals);
    }

    /// The decals alone (the game's roads and footprints change as the town grows).
    pub fn upload_decals(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        decals: &[DecalData],
    ) {
        let mut views: std::collections::HashMap<usize, wgpu::TextureView> = Default::default();
        let white = self.white.clone();
        let mut view = |img: &Option<ImageBuffer>| -> wgpu::TextureView {
            let key = img
                .as_ref()
                .and_then(|i| i.levels.first())
                .map_or(0, |l| l.as_ptr() as usize);
            if key == 0 {
                return white.clone();
            }
            views
                .entry(key)
                .or_insert_with(|| linear_view(device, queue, img, &white))
                .clone()
        };
        self.decals = decals
            .iter()
            .map(|d: &DecalData| {
                let (diffuse, mask, snow) = (view(&d.diffuse), view(&d.mask), view(&d.snow));
                let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("decal"),
                    layout: &self.decal_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&diffuse),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(&mask),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::TextureView(&snow),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: wgpu::BindingResource::Sampler(&self.wrap),
                        },
                    ],
                });
                DecalGpu {
                    kind: d.kind,
                    vertices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("decal vertices"),
                        contents: &d.vertices,
                        usage: wgpu::BufferUsages::VERTEX,
                    }),
                    indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("decal indices"),
                        contents: &d.indices,
                        usage: wgpu::BufferUsages::INDEX,
                    }),
                    index_count: d.index_count,
                    bind,
                }
            })
            .collect();
    }

    fn upload_sky(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        s: &SkyData,
    ) -> Option<SkyGpu> {
        let cube = banished_gfx::upload_cube(device, queue, "sky", &s.cube)?;
        let model = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sky model"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sky"),
            layout: &self.sky_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&cube),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.clamp),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: model.as_entire_binding(),
                },
            ],
        });
        Some(SkyGpu {
            vertices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("sky vertices"),
                contents: &s.vertices,
                usage: wgpu::BufferUsages::VERTEX,
            }),
            indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("sky indices"),
                contents: &s.indices,
                usage: wgpu::BufferUsages::INDEX,
            }),
            index_count: s.index_count,
            model,
            bind,
        })
    }

    fn upload_water(&self, device: &wgpu::Device, queue: &wgpu::Queue, w: &WaterData) -> WaterGpu {
        let view2d = |img: &ImageBuffer| {
            banished_gfx::upload_image_linear(device, queue, "water", img)
                .unwrap_or_else(|| self.white.clone())
        };
        let highlight = banished_gfx::upload_cube(device, queue, "highlight", &w.highlight)
            .unwrap_or_else(|| {
                let t = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("highlight"),
                    size: wgpu::Extent3d {
                        width: 1,
                        height: 1,
                        depth_or_array_layers: 6,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                });
                t.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::Cube),
                    ..Default::default()
                })
            });
        WaterGpu {
            vertices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("water vertices"),
                contents: &w.vertices,
                usage: wgpu::BufferUsages::VERTEX,
            }),
            indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("water indices"),
                contents: &w.indices,
                usage: wgpu::BufferUsages::INDEX,
            }),
            rivers: w.rivers.clone(),
            lakes: w.lakes.clone(),
            river_tex: view2d(&w.river_water),
            lake_tex: view2d(&w.lake_water),
            highlight,
            river_bind: None,
            lake_bind: None,
            level: w.level,
        }
    }

    /// Bind the frame copy and reflection targets (after creation or resize).
    pub fn bind_targets(
        &mut self,
        device: &wgpu::Device,
        frame: &wgpu::TextureView,
        reflection: &wgpu::TextureView,
    ) {
        let Some(w) = &mut self.water else { return };
        let make = |water: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("water"),
                layout: &self.water_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(water),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(frame),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(reflection),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(&w.highlight),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::Sampler(&self.wrap),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: wgpu::BindingResource::Sampler(&self.clamp),
                    },
                ],
            })
        };
        w.river_bind = Some(make(&w.river_tex));
        w.lake_bind = Some(make(&w.lake_tex));
    }

    /// Centre the sky dome on the camera, scaled to stay inside the far plane.
    pub fn place_sky(&self, queue: &wgpu::Queue, eye: glam::Vec3, far: f32) {
        if let Some(s) = &self.sky {
            let radius = far * 0.9 / 0.5;
            let m = glam::Mat4::from_scale_rotation_translation(
                glam::Vec3::splat(radius),
                glam::Quat::IDENTITY,
                glam::Vec3::new(eye.x, eye.y, eye.z - radius * 0.1),
            );
            queue.write_buffer(&s.model, 0, bytemuck::cast_slice(&m.to_cols_array()));
        }
    }

    pub fn draw_sky<'p>(&'p self, pass: &mut wgpu::RenderPass<'p>, globals: &'p wgpu::BindGroup) {
        let Some(s) = &self.sky else { return };
        pass.set_pipeline(&self.sky_pipeline);
        pass.set_bind_group(0, globals, &[]);
        pass.set_bind_group(1, &s.bind, &[]);
        pass.set_vertex_buffer(0, s.vertices.slice(..));
        pass.set_index_buffer(s.indices.slice(..), wgpu::IndexFormat::Uint16);
        pass.draw_indexed(0..s.index_count, 0, 0..1);
    }

    pub fn draw_decals<'p>(
        &'p self,
        pass: &mut wgpu::RenderPass<'p>,
        globals: &'p wgpu::BindGroup,
    ) {
        if self.decals.is_empty() {
            return;
        }
        pass.set_bind_group(0, globals, &[]);
        let mut current = None;
        for d in &self.decals {
            if current != Some(d.kind) {
                let i = match d.kind {
                    DecalKind::Tiled => 0,
                    DecalKind::Blend => 1,
                    DecalKind::Plain => 2,
                };
                pass.set_pipeline(&self.decal_pipelines[i]);
                current = Some(d.kind);
            }
            pass.set_bind_group(1, &d.bind, &[]);
            pass.set_vertex_buffer(0, d.vertices.slice(..));
            pass.set_index_buffer(d.indices.slice(..), wgpu::IndexFormat::Uint16);
            pass.draw_indexed(0..d.index_count, 0, 0..1);
        }
    }

    pub fn draw_water<'p>(&'p self, pass: &mut wgpu::RenderPass<'p>, globals: &'p wgpu::BindGroup) {
        let Some(w) = &self.water else { return };
        let (Some(river), Some(lake)) = (&w.river_bind, &w.lake_bind) else {
            return;
        };
        pass.set_bind_group(0, globals, &[]);
        pass.set_index_buffer(w.indices.slice(..), wgpu::IndexFormat::Uint16);
        for (pipeline, bind, ranges) in [
            (&self.river_pipeline, river, &w.rivers),
            (&self.lake_pipeline, lake, &w.lakes),
        ] {
            pass.set_pipeline(pipeline);
            pass.set_bind_group(1, bind, &[]);
            for &[first_index, first_vertex, vertex_count, index_count] in ranges {
                if index_count <= 0 || vertex_count <= 0 {
                    continue;
                }
                // Indices are local to the range; bind its vertices at an offset (no base vertex
                // on WebGL2).
                let start = first_vertex as u64 * 16;
                let end = start + vertex_count as u64 * 16;
                pass.set_vertex_buffer(0, w.vertices.slice(start..end));
                let first = first_index as u32;
                pass.draw_indexed(first..first + index_count as u32, 0, 0..1);
            }
        }
    }
}
