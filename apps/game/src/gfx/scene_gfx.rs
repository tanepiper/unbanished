//! GPU renderers for the sky, the water surface and the terrain decals, rewritten from the
//! disassembled D3D9 programs (`SkyMaterial`, `RiverMaterial` with its lake vertex shader,
//! `TerrainTiledSnowDecal`, `TerrainDecalBlend`, `TerrainDecal`). See docs/MENU-SCENE.md.

mod decals;
mod pipelines;
mod sky;
mod water;
use decals::{DecalGpu, DECAL_WGSL};
use pipelines::{linear_view, pipeline, sampler_entry, tex_entry, PipelineSpec};
pub use pipelines::{
    material_sampler, multisample, sampler, samples, set_samples, set_texture_filter,
};
use sky::{SkyGpu, SKY_WGSL};
use water::{WaterGpu, WATER_WGSL};

use unbanished_formats::types::ImageBuffer;
use wgpu::util::DeviceExt;

use crate::view::scene_assets::{DecalData, DecalKind, SceneExtras, SkyData, WaterData};

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
        let white = unbanished_gfx::upload_rgba_as(
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
}
