//! wgpu renderer for the game world. Native windows, the web canvas and headless captures share
//! one path. Colours are handled as raw (non-sRGB) values like the original D3D9 renderer.
//!
//! A 3D frame is drawn offscreen like the original pipeline:
//! 0. the sun's shadow map: terrain and model depth from the sun camera (`shadow.rs`);
//! 1. planar reflection (half size): sky, terrain and models mirrored about the water surface,
//!    clipping what lies below it (`ReflectionImageBuffer`);
//! 2. the scene: sky, terrain, models, decals;
//! 3. a copy of the scene for the water shader to refract (`FrameCopyImageBuffer`);
//! 4. the river and lake surface;
//! 5. the scene is copied to the window, then the UI and fades are drawn over it.
//!
//! This file holds the renderer's state and setup; the rest is split by topic: [`frame`]
//! (drawing a frame), [`uploads`], [`minimap`], [`capture`] (reading frames back), [`targets`],
//! [`lighting`] and [`video`] (settings).

mod capture;
mod frame;
mod lighting;
mod minimap;
mod targets;
mod uploads;
mod video;
use video::{present_mode, BACKENDS};

use std::sync::Arc;

use banished_formats::texture::decode_rgba;
use banished_formats::types::ImageBuffer;
use banished_game::terrain::TILE_VERTS;
use glam::{Mat4, Vec3, Vec4};
use wgpu::util::DeviceExt;
use winit::window::Window;

use crate::gfx::scene_gfx::{SceneExtrasRenderer, DEPTH_FORMAT, GLOBALS_WGSL, SCENE_FORMAT};
use crate::view::world::Terrain;

const CAPTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// Matches `scene_gfx::GLOBALS_WGSL`.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    light_dir: [f32; 4],
    light_color: [f32; 4],
    ambient_color: [f32; 4],
    fog_color: [f32; 4],
    fog: [f32; 4],
    user: [f32; 4],
    camera_right: [f32; 4],
    camera_forward: [f32; 4],
    camera_up: [f32; 4],
    camera_pos: [f32; 4],
    clip: [f32; 4],
    shadow: [[f32; 4]; 4],
    shadow_params: [f32; 4],
}

/// Lighting for one frame, in the units of `EnvironmentDescription`.
#[derive(Clone, Copy)]
pub struct Lighting {
    /// Direction the sunlight travels.
    pub light_dir: Vec3,
    pub light_color: Vec3,
    pub ambient_color: Vec3,
    pub fog_color: Vec3,
    pub fog_max: f32,
    pub fog_start: f32,
    pub fog_end: f32,
    /// `pc_userPosition`: x snow, y autumn colour, z leaf loss, w rain (menu scenes set it;
    /// in game the climate does).
    pub user: Vec4,
}

/// Camera and lighting of a 3D frame.
#[derive(Clone, Copy)]
pub struct SceneFrame {
    pub view_proj: Mat4,
    pub lighting: Lighting,
    pub eye: Vec3,
    pub far: f32,
    pub right: Vec3,
    pub forward: Vec3,
    pub up: Vec3,
    pub time: f32,
    /// Horizontal and vertical field of view (degrees) and near distance of the projection,
    /// for the sun camera fit.
    pub hfov: f32,
    pub vfov: f32,
    pub near: f32,
}

/// Everything drawn in one frame: an optional 3D scene, then UI quads.
pub struct Frame<'a> {
    pub terrain: Option<SceneFrame>,
    pub clear: wgpu::Color,
    pub ui: Option<(&'a banished_ui::Ui, &'a [banished_ui::Quad])>,
}

struct TerrainGpu {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    index_count: u32,
    tiles: u32,
    bind_group: wgpu::BindGroup,
    /// Each tile's box (lowest and highest corner), for skipping tiles a pass cannot see.
    bounds: Vec<(Vec3, Vec3)>,
    /// The tiles each [`ModelPass`](crate::gfx::models::ModelPass) draws this frame.
    visible: [Vec<bool>; 3],
}

/// The box around each tile's vertices, in the terrain shader's world space (`x`, `y` in
/// cells, the height / 256).
fn tile_bounds(mesh: &banished_game::terrain::TerrainMesh) -> Vec<(Vec3, Vec3)> {
    mesh.vertices
        .chunks(TILE_VERTS * TILE_VERTS)
        .map(|tile| {
            tile.iter().fold(
                (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
                |(lo, hi), v| {
                    let p = Vec3::new(v.pos[0] as f32, v.pos[1] as f32, v.pos[2] as f32 / 256.0);
                    (lo.min(p), hi.max(p))
                },
            )
        })
        .collect()
}

/// Offscreen render targets, recreated on resize.
struct Targets {
    scene: wgpu::Texture,
    scene_view: wgpu::TextureView,
    frame_copy: wgpu::Texture,
    frame_copy_view: wgpu::TextureView,
    reflection_view: wgpu::TextureView,
    reflection_depth: wgpu::TextureView,
    depth: wgpu::TextureView,
    blit_bind: wgpu::BindGroup,
    /// With antialiasing (`Video::Configuration +0x24`), the multisampled colour the scene and
    /// the reflection are drawn into, resolved into `scene` and `reflection`; `depth` and
    /// `reflection_depth` are then multisampled too.
    scene_draw: Option<wgpu::TextureView>,
    reflection_draw: Option<wgpu::TextureView>,
    /// The single-sampled depth of the screen and minimap passes (the blit and the UI).
    screen_depth: wgpu::TextureView,
}

pub struct Renderer {
    /// Wait for the GPU when reading back a save's preview, so the save is written on the
    /// frame it was asked for. Headless captures set it: without it the number of frames a
    /// save waits depends on GPU timing, and anything that changes while the game waits (the
    /// growth smoothing runs even when paused) makes two identical runs save different states.
    pub blocking_readback: bool,
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: Option<(wgpu::Surface<'static>, wgpu::SurfaceConfiguration)>,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    format: wgpu::TextureFormat,
    size: (u32, u32),
    /// The 3D scene's size: the window's times [`Renderer::set_scene_scale`]'s factor. The
    /// scene's targets have this size; the copy to the window scales it up, and the
    /// interface is drawn at the window's size.
    scene_size: (u32, u32),
    scene_scale: f32,
    targets: Targets,
    terrain_pipeline: wgpu::RenderPipeline,
    terrain_depth_pipeline: wgpu::RenderPipeline,
    shadow: crate::gfx::shadow::ShadowMap,
    shadow_globals: wgpu::Buffer,
    shadow_bind: wgpu::BindGroup,
    blit_pipeline: wgpu::RenderPipeline,
    blit_layout: wgpu::BindGroupLayout,
    blit_sampler: wgpu::Sampler,
    globals: wgpu::Buffer,
    globals_bind: wgpu::BindGroup,
    reflect_globals: wgpu::Buffer,
    reflect_bind: wgpu::BindGroup,
    material_layout: wgpu::BindGroupLayout,
    terrain: Option<TerrainGpu>,
    /// `SceneView::id` of the uploaded scene.
    pub world_id: u64,
    /// The terrain's height revision last uploaded.
    terrain_revision: u64,
    /// What the Options dialog may choose from (display modes are added by the platform).
    pub display: crate::settings::DisplayInfo,
    pub video: VideoSettings,
    models_revision: u64,
    ui: banished_gfx::UiRenderer,
    models: crate::gfx::models::ModelRenderer,
    extras: SceneExtrasRenderer,
    decals_revision: u64,
    highlight: crate::gfx::highlight::HighlightRenderer,
    status_icons: crate::gfx::status_icons::StatusIconRenderer,
    particles: crate::gfx::particle_gfx::ParticleRenderer,
    weather: crate::gfx::particle_gfx::WeatherRenderer,
    pending_weather: Vec<crate::view::weather::WeatherBatch>,
    weather_textures:
        std::collections::HashMap<String, Option<banished_formats::types::ImageBuffer>>,
    cell_overlay: crate::gfx::selection_overlay::OverlayRenderer,
    pending_overlay: Vec<crate::gfx::selection_overlay::OverlayVertex>,
    overlay_texture: Option<crate::gfx::selection_overlay::OverlayTexture>,
    pending_icons: Vec<crate::gfx::status_icons::StatusIcon>,
    /// Game options `StatusIconOpacity`, `StatusIconEnable`, `StatusIconScale`.
    pub icon_options: crate::gfx::status_icons::IconOptions,
    icon_sheets: std::collections::BTreeMap<String, crate::gfx::status_icons::IconSheet>,
    /// The minimap image (rendered once per world) and the overlay revision uploaded.
    minimap: Option<wgpu::Texture>,
    minimap_overlay: u64,
    /// The sun of the last scene drawn: `SunLight::Draw`'s `Context::SetLight` constants,
    /// which `Context::Reset` leaves (it clears only the light block at `+0x1dd8`).
    last_light: Option<Lighting>,
    pub backend: String,
}

fn texture(
    device: &wgpu::Device,
    label: &str,
    size: (u32, u32),
    format: wgpu::TextureFormat,
    usage: wgpu::TextureUsages,
) -> wgpu::Texture {
    texture_samples(device, label, size, format, usage, 1)
}

fn texture_samples(
    device: &wgpu::Device,
    label: &str,
    (w, h): (u32, u32),
    format: wgpu::TextureFormat,
    usage: wgpu::TextureUsages,
    samples: u32,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: w.max(1),
            height: h.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: samples,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage,
        view_formats: &[],
    })
}

/// A pass that only resolves `draw` (multisampled) into `into`.
fn resolve(encoder: &mut wgpu::CommandEncoder, draw: &wgpu::TextureView, into: &wgpu::TextureView) {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("resolve"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: draw,
            depth_slice: None,
            resolve_target: Some(into),
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Load,
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
}

fn color(argb: u32) -> Vec3 {
    let c = |s: u32| ((argb >> s) & 0xff) as f32 / 255.0;
    Vec3::new(c(16), c(8), c(0))
}

const BLIT_WGSL: &str = r#"
@group(0) @binding(0) var scene: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;

struct BlitOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_blit(@builtin(vertex_index) i: u32) -> BlitOut {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var o: BlitOut;
    o.pos = vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
    o.uv = vec2<f32>(p.x, 1.0 - p.y);
    return o;
}

@fragment
fn fs_blit(i: BlitOut) -> @location(0) vec4<f32> {
    return vec4<f32>(textureSample(scene, samp, i.uv).rgb, 1.0);
}
"#;

/// The video configuration the renderer is built with (`Video::Configuration`). Interface,
/// adapter, filtering and shadow resolution take effect when the renderer is created; the rest
/// with [`Renderer::apply_video`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VideoSettings {
    /// Index into [`Renderer::display`]'s interfaces (wgpu backends).
    pub interface: i32,
    pub adapter: i32,
    /// "TextureFilter": −1 bilinear, 0 trilinear, n anisotropic.
    pub filter: i32,
    /// "ShadowScalar": the shadow map is 2048 >> scalar.
    pub shadow_scalar: i32,
    /// "ShaderDetail": 0 no shadows, 1/2/3 one, four or nine shadow taps.
    pub shadow_detail: i32,
    /// "ReflectionDetail": 0 sky only, 1 terrain, 2 everything in the water reflection.
    pub reflection: i32,
    pub vsync: bool,
    /// "Multisample": 0 none, n for n + 1 samples.
    pub multisample: i32,
}

/// A texture copied to a buffer whose mapping completes later (the browser cannot wait on
/// the GPU): `poll` gives the texels once mapped.
struct Readback {
    buffer: wgpu::Buffer,
    /// 0 waiting, 1 mapped, 2 failed.
    mapped: std::sync::Arc<std::sync::atomic::AtomicU8>,
    width: u32,
    height: u32,
    row: u32,
    bgra: bool,
}

/// A save's frame and minimap being read back (`SaveLoad::Draw`, `FUN_1400aa610`).
pub struct PendingCapture {
    frame: Readback,
    minimap: Option<Readback>,
}

impl Renderer {
    pub async fn new(window: Arc<Window>, video: VideoSettings) -> Renderer {
        let size = window.inner_size();
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let (instance, surface) = match instance.create_surface(window.clone()) {
            Ok(surface) => (instance, surface),
            // A browser can offer WebGPU (`navigator.gpu`) and still give the canvas no WebGPU
            // context (headless Firefox, a blocked GPU). The canvas has no context yet, so it
            // can still give a WebGL 2 one.
            #[cfg(target_arch = "wasm32")]
            Err(e) => {
                log::warn!("no WebGPU canvas ({e}); using WebGL 2");
                let mut desc = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
                desc.backends = wgpu::Backends::GL;
                let gl = wgpu::Instance::new(desc);
                let surface = gl.create_surface(window).expect("create surface");
                (gl, surface)
            }
            #[cfg(not(target_arch = "wasm32"))]
            Err(e) => panic!("create surface: {e}"),
        };
        Self::init(
            &instance,
            Some(surface),
            size.width.max(1),
            size.height.max(1),
            video,
        )
        .await
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub async fn headless(width: u32, height: u32) -> Renderer {
        Self::headless_with(width, height, VideoSettings::default()).await
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub async fn headless_with(width: u32, height: u32, video: VideoSettings) -> Renderer {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        Self::init(&instance, None, width, height, video).await
    }

    /// The adapters usable for `surface`, grouped by backend in [`BACKENDS`] order.
    async fn interfaces(
        instance: &wgpu::Instance,
        surface: Option<&wgpu::Surface<'static>>,
    ) -> Vec<(&'static str, Vec<wgpu::Adapter>)> {
        let all = instance.enumerate_adapters(wgpu::Backends::all()).await;
        BACKENDS
            .iter()
            .filter_map(|&(backend, name)| {
                let adapters: Vec<wgpu::Adapter> = all
                    .iter()
                    .filter(|a| a.get_info().backend == backend)
                    .filter(|a| surface.is_none_or(|s| a.is_surface_supported(s)))
                    .cloned()
                    .collect();
                (!adapters.is_empty()).then_some((name, adapters))
            })
            .collect()
    }

    async fn init(
        instance: &wgpu::Instance,
        surface: Option<wgpu::Surface<'static>>,
        width: u32,
        height: u32,
        video: VideoSettings,
    ) -> Renderer {
        let interfaces = Self::interfaces(instance, surface.as_ref()).await;
        let chosen = interfaces
            .get(video.interface.max(0) as usize)
            .and_then(|(_, adapters)| adapters.get(video.adapter.max(0) as usize))
            .cloned();
        let adapter = match chosen {
            Some(adapter) => adapter,
            None => instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    compatible_surface: surface.as_ref(),
                    force_fallback_adapter: false,
                    apply_limit_buckets: false,
                })
                .await
                .expect("no suitable GPU adapter"),
        };
        crate::gfx::scene_gfx::set_texture_filter(video.filter);
        let info = adapter.get_info();
        let backend = format!("{:?} ({})", info.backend, info.name);
        log::info!("adapter: {backend}");
        // Counts beyond WebGPU's guaranteed 1 and 4 need the adapter's own format features.
        let specific = adapter
            .features()
            .contains(wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES);
        let display = crate::settings::DisplayInfo {
            interfaces: interfaces
                .iter()
                .map(|(name, adapters)| crate::settings::GraphicsInterface {
                    name: name.to_string(),
                    adapters: adapters
                        .iter()
                        .map(|a| crate::settings::Adapter {
                            name: a.get_info().name,
                            modes: Vec::new(),
                        })
                        .collect(),
                })
                .collect(),
            // `IsMultisampleModeSupported`: entry `n` is `n + 1` samples, as the adapter takes
            // them for the scene's colour and depth.
            multisample: (1..=16u32)
                .map(|n| {
                    n == 1
                        || (n == 4 || specific)
                            && [SCENE_FORMAT, DEPTH_FORMAT].iter().all(|f| {
                                adapter
                                    .get_texture_format_features(*f)
                                    .flags
                                    .sample_count_supported(n)
                            })
                })
                .collect(),
            max_anisotropy: if adapter
                .get_downlevel_capabilities()
                .flags
                .contains(wgpu::DownlevelFlags::ANISOTROPIC_FILTERING)
            {
                16
            } else {
                0
            },
            desktop: None,
        };
        // The antialiasing chosen (`Video::Configuration +0x24`: n for n + 1 samples), when
        // the adapter supports it; every scene pipeline below is made with it.
        let samples = usize::try_from(video.multisample)
            .ok()
            .filter(|&n| display.multisample.get(n).copied().unwrap_or(false))
            .map_or(1, |n| n as u32 + 1);
        crate::gfx::scene_gfx::set_samples(samples);
        log::info!("antialiasing: {samples} samples");
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("game"),
                required_features: if specific {
                    wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES
                } else {
                    wgpu::Features::empty()
                },
                required_limits: wgpu::Limits::downlevel_webgl2_defaults()
                    .using_resolution(adapter.limits()),
                ..Default::default()
            })
            .await
            .expect("request device");

        let surface = surface.map(|surface| {
            let mut config = surface
                .get_default_config(&adapter, width, height)
                .expect("surface unsupported by adapter");
            let caps = surface.get_capabilities(&adapter);
            if let Some(linear) = caps.formats.iter().find(|f| !f.is_srgb()) {
                config.format = *linear;
            }
            config.present_mode = present_mode(video.vsync);
            surface.configure(&device, &config);
            (surface, config)
        });
        let format = surface.as_ref().map_or(CAPTURE_FORMAT, |(_, c)| c.format);

        let [shadow_map_entry, shadow_sampler_entry] =
            crate::gfx::shadow::ShadowMap::layout_entries();
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                shadow_map_entry,
                shadow_sampler_entry,
            ],
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
        let sampler = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        };
        let material_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("terrain material"),
            entries: &[
                tex(0),
                tex(1),
                tex(2),
                tex(3),
                tex(4),
                sampler(5),
                sampler(6),
            ],
        });

        let terrain_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("terrain"),
            source: wgpu::ShaderSource::Wgsl(
                format!("{GLOBALS_WGSL}{}", include_str!("terrain.wgsl")).into(),
            ),
        });
        let terrain_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("terrain"),
            bind_group_layouts: &[Some(&globals_layout), Some(&material_layout)],
            immediate_size: 0,
        });
        let terrain_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("terrain"),
            layout: Some(&terrain_layout),
            vertex: wgpu::VertexState {
                module: &terrain_shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: 16,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Sint16x4, 1 => Sint16x2, 2 => Unorm8x4
                    ],
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &terrain_shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: SCENE_FORMAT,
                    blend: None,
                    // Alpha carries the underwater factor for the water shader.
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
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: crate::gfx::scene_gfx::multisample(),
            multiview_mask: None,
            cache: None,
        });
        // TerrainMaterial_Depth_vs places vertices as Normal_vs does.
        let terrain_depth_pipeline = crate::gfx::shadow::depth_pipeline(
            &device,
            "terrain depth",
            &terrain_layout,
            &terrain_shader,
            "vs_main",
            None,
            &[Some(wgpu::VertexBufferLayout {
                array_stride: 16,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Sint16x4, 1 => Sint16x2, 2 => Unorm8x4],
            })],
            crate::gfx::shadow::DepthState::BackFaces,
        );

        let blit_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blit"),
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
        let blit_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("blit"),
            source: wgpu::ShaderSource::Wgsl(BLIT_WGSL.into()),
        });
        let blit_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("blit"),
            layout: Some(
                &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("blit"),
                    bind_group_layouts: &[Some(&blit_layout)],
                    immediate_size: 0,
                }),
            ),
            vertex: wgpu::VertexState {
                module: &blit_shader,
                entry_point: Some("vs_blit"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &blit_shader,
                entry_point: Some("fs_blit"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let blit_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let shadow = crate::gfx::shadow::ShadowMap::new(
            &device,
            crate::gfx::shadow::shadow_size(video.shadow_scalar),
        );
        let globals_buffer = |label, shadow_view: &wgpu::TextureView| {
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: std::mem::size_of::<Globals>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(label),
                layout: &globals_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(shadow_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&shadow.sampler),
                    },
                ],
            });
            (buffer, bind)
        };
        let (globals, globals_bind) = globals_buffer("globals", &shadow.view);
        let (reflect_globals, reflect_bind) = globals_buffer("reflection globals", &shadow.view);
        // The shadow pass renders into the map, so its globals bind a stand-in.
        let (shadow_globals, shadow_bind) = globals_buffer("shadow globals", &shadow.dummy);
        let models = crate::gfx::models::ModelRenderer::new(&device, &queue, &globals_layout);
        let extras = SceneExtrasRenderer::new(&device, &queue, &globals_layout);
        let highlight = crate::gfx::highlight::HighlightRenderer::new(&device);
        let status_icons = crate::gfx::status_icons::StatusIconRenderer::new(&device);
        let particles =
            crate::gfx::particle_gfx::ParticleRenderer::new(&device, &queue, &globals_layout);
        let weather =
            crate::gfx::particle_gfx::WeatherRenderer::new(&device, &queue, &globals_layout);
        let cell_overlay = crate::gfx::selection_overlay::OverlayRenderer::new(&device);
        let mut ui = banished_gfx::UiRenderer::new(&device, format, DEPTH_FORMAT);
        let white = banished_gfx::upload_rgba_as(
            &device,
            &queue,
            "white",
            1,
            1,
            &[vec![255; 4]],
            wgpu::TextureFormat::Rgba8Unorm,
        );
        ui.set_external(&device, crate::loops::WHITE, &white);
        let targets = Targets::new(
            &device,
            (width, height),
            (width, height),
            &blit_layout,
            &blit_sampler,
        );
        Renderer {
            blocking_readback: false,
            device,
            queue,
            surface,
            format,
            size: (width, height),
            scene_size: (width, height),
            scene_scale: 1.0,
            targets,
            terrain_pipeline,
            terrain_depth_pipeline,
            shadow,
            shadow_globals,
            shadow_bind,
            blit_pipeline,
            blit_layout,
            blit_sampler,
            globals,
            globals_bind,
            reflect_globals,
            reflect_bind,
            material_layout,
            terrain: None,
            world_id: 0,
            terrain_revision: 0,
            models_revision: 0,
            ui,
            models,
            highlight,
            status_icons,
            particles,
            weather,
            pending_weather: Vec::new(),
            weather_textures: Default::default(),
            cell_overlay,
            pending_overlay: Vec::new(),
            overlay_texture: None,
            pending_icons: Vec::new(),
            icon_options: crate::gfx::status_icons::IconOptions::default(),
            icon_sheets: Default::default(),
            extras,
            decals_revision: 0,
            minimap: None,
            minimap_overlay: 0,
            last_light: None,
            backend,
            display,
            video,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        let (width, height) = (width.max(1), height.max(1));
        if self.size == (width, height) {
            return;
        }
        self.size = (width, height);
        if let Some((surface, config)) = &mut self.surface {
            config.width = width;
            config.height = height;
            surface.configure(&self.device, config);
        }
        self.resize_scene();
    }

    /// Draw the 3D scene at `scale` (0.25 to 1) of the window's size and scale it up to the
    /// window; the interface stays at the window's size. Deviation: a browser setting (the
    /// Options resolution list there), so slower devices can draw fewer pixels; natively
    /// the scale is always 1.
    pub fn set_scene_scale(&mut self, scale: f32) {
        let scale = scale.clamp(0.25, 1.0);
        if scale != self.scene_scale {
            self.scene_scale = scale;
            self.resize_scene();
        }
    }

    fn resize_scene(&mut self) {
        let s = self.scene_scale;
        let size = (
            ((self.size.0 as f32 * s).round() as u32).max(1),
            ((self.size.1 as f32 * s).round() as u32).max(1),
        );
        if size == self.scene_size && self.targets_size() == size && !self.screen_resized() {
            return;
        }
        self.scene_size = size;
        self.targets = Targets::new(
            &self.device,
            self.scene_size,
            self.size,
            &self.blit_layout,
            &self.blit_sampler,
        );
        self.extras.bind_targets(
            &self.device,
            &self.targets.frame_copy_view,
            &self.targets.reflection_view,
        );
    }

    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    fn targets_size(&self) -> (u32, u32) {
        let s = self.targets.scene.size();
        (s.width, s.height)
    }

    /// Whether the window's size differs from the screen depth's (made with the targets).
    fn screen_resized(&self) -> bool {
        let s = self.targets.screen_depth.texture().size();
        (s.width, s.height) != self.size
    }

    pub fn aspect(&self) -> f32 {
        self.size.0 as f32 / self.size.1 as f32
    }
}
