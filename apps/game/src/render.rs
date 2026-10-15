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

use std::sync::Arc;

use banished_formats::texture::decode_rgba;
use banished_formats::types::ImageBuffer;
use banished_game::terrain::TILE_VERTS;
use glam::{Mat4, Vec3, Vec4};
use wgpu::util::DeviceExt;
use winit::window::Window;

use crate::scene_gfx::{SceneExtrasRenderer, DEPTH_FORMAT, GLOBALS_WGSL, SCENE_FORMAT};
use crate::world::Terrain;

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
}

pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: Option<(wgpu::Surface<'static>, wgpu::SurfaceConfiguration)>,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    format: wgpu::TextureFormat,
    size: (u32, u32),
    targets: Targets,
    terrain_pipeline: wgpu::RenderPipeline,
    terrain_depth_pipeline: wgpu::RenderPipeline,
    shadow: crate::shadow::ShadowMap,
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
    models_revision: u64,
    ui: banished_gfx::UiRenderer,
    models: crate::models::ModelRenderer,
    extras: SceneExtrasRenderer,
    highlight: crate::highlight::HighlightRenderer,
    status_icons: crate::status_icons::StatusIconRenderer,
    pending_icons: Vec<crate::status_icons::StatusIcon>,
    icon_sheets: std::collections::BTreeMap<String, crate::status_icons::IconSheet>,
    /// The minimap image (rendered once per world) and the overlay revision uploaded.
    minimap: Option<wgpu::Texture>,
    minimap_overlay: u64,
    pub backend: String,
}

fn texture(
    device: &wgpu::Device,
    label: &str,
    (w, h): (u32, u32),
    format: wgpu::TextureFormat,
    usage: wgpu::TextureUsages,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: w.max(1),
            height: h.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage,
        view_formats: &[],
    })
}

impl Targets {
    fn new(
        device: &wgpu::Device,
        size: (u32, u32),
        blit_layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
    ) -> Targets {
        use wgpu::TextureUsages as U;
        let scene = texture(
            device,
            "scene",
            size,
            SCENE_FORMAT,
            U::RENDER_ATTACHMENT | U::TEXTURE_BINDING | U::COPY_SRC,
        );
        let frame_copy = texture(
            device,
            "frame copy",
            size,
            SCENE_FORMAT,
            U::TEXTURE_BINDING | U::COPY_DST,
        );
        let half = ((size.0 / 2).max(1), (size.1 / 2).max(1));
        let reflection = texture(
            device,
            "reflection",
            half,
            SCENE_FORMAT,
            U::RENDER_ATTACHMENT | U::TEXTURE_BINDING,
        );
        let view = |t: &wgpu::Texture| t.create_view(&Default::default());
        let scene_view = view(&scene);
        let blit_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blit"),
            layout: blit_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&scene_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });
        Targets {
            frame_copy_view: view(&frame_copy),
            frame_copy,
            reflection_view: view(&reflection),
            reflection_depth: view(&texture(
                device,
                "reflection depth",
                half,
                DEPTH_FORMAT,
                U::RENDER_ATTACHMENT,
            )),
            depth: view(&texture(
                device,
                "depth",
                size,
                DEPTH_FORMAT,
                U::RENDER_ATTACHMENT,
            )),
            scene_view,
            scene,
            blit_bind,
        }
    }
}

fn color(argb: u32) -> Vec3 {
    let c = |s: u32| ((argb >> s) & 0xff) as f32 / 255.0;
    Vec3::new(c(16), c(8), c(0))
}

impl Lighting {
    /// One month of `EnvironmentDescription`, as `FUN_140098da0` applies it.
    pub fn from_month(m: &banished_formats::game::EnvironmentMonth) -> Lighting {
        Lighting::between_months(m, m, 0.0)
    }

    /// Interpolate adjacent month records as `FUN_140098f90` does before applying the sun and fog.
    pub fn between_months(
        from: &banished_formats::game::EnvironmentMonth,
        to: &banished_formats::game::EnvironmentMonth,
        amount: f32,
    ) -> Lighting {
        let amount = amount.clamp(0.0, 1.0);
        let blend = |a: f32, b: f32| a + (b - a) * amount;
        let (yaw, pitch) = (
            blend(from.sun_yaw, to.sun_yaw).to_radians(),
            blend(from.sun_pitch, to.sun_pitch).to_radians(),
        );
        // Math::Vector3::FromYawPitch points towards the sun; the shader wants the travel
        // direction (inferred from the sign of its diffuse term).
        let to_sun = Vec3::new(
            -pitch.cos() * yaw.sin(),
            pitch.cos() * yaw.cos(),
            pitch.sin(),
        );
        Lighting {
            light_dir: -to_sun,
            light_color: color(from.sun_color).lerp(color(to.sun_color), amount),
            ambient_color: color(from.ambient_color).lerp(color(to.ambient_color), amount),
            fog_color: color(from.fog_color).lerp(color(to.fog_color), amount),
            fog_max: blend(from.fog_max, to.fog_max),
            fog_start: blend(from.fog_start, to.fog_start),
            fog_end: blend(from.fog_end, to.fog_end),
            user: Vec4::ZERO,
        }
    }

    /// Apply the date-driven foliage controls computed by `FUN_1400988f0`.
    pub fn set_seasonal_foliage(&mut self, month_position: f32) {
        self.user.y = ((month_position - 6.0) / 1.5).clamp(0.0, 1.0);
        self.user.z = if month_position < 0.5 {
            (1.0 - month_position / 0.5).clamp(0.0, 1.0)
        } else if month_position <= 7.5 {
            0.0
        } else {
            ((month_position - 7.5) * 2.0).clamp(0.0, 1.0)
        };
    }
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

impl Renderer {
    pub async fn new(window: Arc<Window>) -> Renderer {
        let size = window.inner_size();
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let surface = instance.create_surface(window).expect("create surface");
        Self::init(
            &instance,
            Some(surface),
            size.width.max(1),
            size.height.max(1),
        )
        .await
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub async fn headless(width: u32, height: u32) -> Renderer {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        Self::init(&instance, None, width, height).await
    }

    async fn init(
        instance: &wgpu::Instance,
        surface: Option<wgpu::Surface<'static>>,
        width: u32,
        height: u32,
    ) -> Renderer {
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: surface.as_ref(),
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            })
            .await
            .expect("no suitable GPU adapter");
        let info = adapter.get_info();
        let backend = format!("{:?} ({})", info.backend, info.name);
        log::info!("adapter: {backend}");
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("game"),
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
            surface.configure(&device, &config);
            (surface, config)
        });
        let format = surface.as_ref().map_or(CAPTURE_FORMAT, |(_, c)| c.format);

        let [shadow_map_entry, shadow_sampler_entry] = crate::shadow::ShadowMap::layout_entries();
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
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        // TerrainMaterial_Depth_vs places vertices as Normal_vs does.
        let terrain_depth_pipeline = crate::shadow::depth_pipeline(
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
            crate::shadow::DepthState::BackFaces,
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

        let shadow = crate::shadow::ShadowMap::new(&device);
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
        let models = crate::models::ModelRenderer::new(&device, &queue, &globals_layout);
        let extras = SceneExtrasRenderer::new(&device, &queue, &globals_layout);
        let highlight = crate::highlight::HighlightRenderer::new(&device);
        let status_icons = crate::status_icons::StatusIconRenderer::new(&device);
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
        let targets = Targets::new(&device, (width, height), &blit_layout, &blit_sampler);
        Renderer {
            device,
            queue,
            surface,
            format,
            size: (width, height),
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
            models_revision: 0,
            ui,
            models,
            highlight,
            status_icons,
            pending_icons: Vec::new(),
            icon_sheets: Default::default(),
            extras,
            minimap: None,
            minimap_overlay: 0,
            backend,
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
        self.targets = Targets::new(
            &self.device,
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

    /// Seconds since the current loop started (animated UI materials).
    pub fn set_ui_time(&mut self, seconds: f32) {
        self.ui.set_time(seconds);
    }

    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    pub fn aspect(&self) -> f32 {
        self.size.0 as f32 / self.size.1 as f32
    }

    fn upload(
        &self,
        label: &str,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        mips: &[Vec<u8>],
    ) -> wgpu::TextureView {
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: mips.len() as u32,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for (level, data) in mips.iter().enumerate() {
            let (w, h) = ((width >> level).max(1), (height >> level).max(1));
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: level as u32,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(w * 4),
                    rows_per_image: Some(h),
                },
                wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
            );
        }
        texture.create_view(&Default::default())
    }

    fn upload_image(&self, label: &str, img: &ImageBuffer) -> wgpu::TextureView {
        let mips: Vec<Vec<u8>> = (0..img.mip_count().max(1))
            .map(|i| {
                decode_rgba(
                    img.format,
                    (img.width >> i).max(1),
                    (img.height >> i).max(1),
                    &img.levels[i],
                )
                .unwrap_or_else(|| {
                    vec![255; (((img.width >> i).max(1)) * ((img.height >> i).max(1)) * 4) as usize]
                })
            })
            .collect();
        self.upload(
            label,
            img.width,
            img.height,
            wgpu::TextureFormat::Rgba8Unorm,
            &mips,
        )
    }

    /// Upload a scene's terrain, models, sky, water and decals if it is not the one on the GPU.
    pub fn set_scene(&mut self, view: &crate::scene::SceneView) {
        self.highlight.upload(&self.device, &view.highlight);
        self.pending_icons.clone_from(&view.status_icons);
        for (k, v) in &view.icon_sheets {
            if !self.icon_sheets.contains_key(k) {
                self.icon_sheets.insert(k.clone(), v.clone());
            }
        }
        if self.world_id != view.id {
            self.set_terrain(&view.terrain);
            match &view.models {
                Some(m) => self.models.upload(&self.device, &self.queue, m),
                None => self.models.clear(),
            }
            match &view.extras {
                Some(e) => self.extras.upload(&self.device, &self.queue, e),
                None => self.extras.clear(),
            }
            self.extras.bind_targets(
                &self.device,
                &self.targets.frame_copy_view,
                &self.targets.reflection_view,
            );
            self.world_id = view.id;
            self.models_revision = view.models.as_ref().map_or(0, |m| m.revision);
            self.minimap = None;
            self.minimap_overlay = 0;
        } else if let Some(models) = &view.models {
            if self.models_revision != models.revision {
                self.models
                    .update_instances(&self.device, &self.queue, models);
                self.models_revision = models.revision;
            }
        }
    }

    /// MapUI's images. `FUN_1400aa610` renders the terrain and water geometries once, on the
    /// first game draw: an orthographic camera over the map (±size/2, near 0, far 512) at
    /// height 100 looking straight down (pitch -90), the light from directly above, fog pushed
    /// to 1000–1001, into a size×size image. The overlay (`FUN_1400aa5b0`) is uploaded when it
    /// changes.
    pub fn update_minimap(&mut self, view: &crate::scene::SceneView) {
        let Some((revision, size, rgba)) = &view.minimap_overlay else {
            return;
        };
        if self.minimap.is_none() && self.terrain.is_some() {
            self.render_minimap(*size, &view.lighting);
        }
        if self.minimap_overlay != *revision {
            let texture = banished_gfx::upload_rgba_as(
                &self.device,
                &self.queue,
                "minimap overlay",
                *size,
                *size,
                std::slice::from_ref(rgba),
                wgpu::TextureFormat::Rgba8Unorm,
            );
            self.ui
                .set_external(&self.device, crate::loops::MINIMAP_OVERLAY, &texture);
            self.minimap_overlay = *revision;
        }
    }

    fn render_minimap(&mut self, n: u32, lighting: &Lighting) {
        let targets = Targets::new(&self.device, (n, n), &self.blit_layout, &self.blit_sampler);
        let half = n as f32 * 0.5;
        let eye = Vec3::new(0.0, 0.0, 100.0);
        use glam::camera::rh::{proj::directx::orthographic, view::look_at_mat4};
        let view_proj = orthographic(-half, half, -half, half, 0.0, 512.0)
            * look_at_mat4(eye, Vec3::ZERO, Vec3::Y);
        let mut lighting = *lighting;
        lighting.light_dir = Vec3::NEG_Z;
        lighting.fog_start = 1000.0;
        lighting.fog_end = 1001.0;
        lighting.fog_color = Vec3::ONE;
        let scene = SceneFrame {
            view_proj,
            lighting,
            eye,
            far: 512.0,
            right: Vec3::X,
            forward: Vec3::NEG_Z,
            up: Vec3::Y,
            time: 0.0,
            hfov: 0.0,
            vfov: 0.0,
            near: 0.0,
        };
        // The minimap scene has no sun light, so no shadow map.
        self.shadow.lookup = None;
        self.extras.bind_targets(
            &self.device,
            &targets.frame_copy_view,
            &targets.reflection_view,
        );
        let water = self.extras.water_level();
        self.write_globals(&self.globals, &scene, None);
        if let Some(level) = water {
            self.write_globals(&self.reflect_globals, &scene, Some(level));
        }
        let output = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("minimap"),
            size: wgpu::Extent3d {
                width: n,
                height: n,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let output_view = output.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        // `Context::Clear(7, 0xffff00ff)`.
        let clear = wgpu::Color {
            r: 1.0,
            g: 0.0,
            b: 1.0,
            a: 1.0,
        };
        if water.is_some() {
            let mut pass = Self::scene_pass(
                &mut encoder,
                "minimap reflection",
                &targets.reflection_view,
                &targets.reflection_depth,
                Some(clear),
            );
            self.draw_terrain(&mut pass, &self.reflect_bind);
        }
        {
            let mut pass = Self::scene_pass(
                &mut encoder,
                "minimap",
                &targets.scene_view,
                &targets.depth,
                Some(clear),
            );
            self.draw_terrain(&mut pass, &self.globals_bind);
        }
        if water.is_some() {
            encoder.copy_texture_to_texture(
                targets.scene.as_image_copy(),
                targets.frame_copy.as_image_copy(),
                wgpu::Extent3d {
                    width: n,
                    height: n,
                    depth_or_array_layers: 1,
                },
            );
            let mut pass = Self::scene_pass(
                &mut encoder,
                "minimap water",
                &targets.scene_view,
                &targets.depth,
                None,
            );
            self.extras.draw_water(&mut pass, &self.globals_bind);
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("minimap blit"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &output_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &targets.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.blit_pipeline);
            pass.set_bind_group(0, &targets.blit_bind, &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit([encoder.finish()]);
        self.extras.bind_targets(
            &self.device,
            &self.targets.frame_copy_view,
            &self.targets.reflection_view,
        );
        self.ui
            .set_external(&self.device, crate::loops::MINIMAP, &output_view);
        self.minimap = Some(output);
    }

    /// Upload a terrain's mesh, surface image and textures.
    pub fn set_terrain(&mut self, world: &Terrain) {
        let surface = world.surface.clone();
        let size = world.size as u32;
        let views = [
            self.upload_image("grass", &world.textures.grass),
            self.upload_image("dirt", &world.textures.dirt),
            self.upload_image("snow", &world.textures.snow),
            self.upload_image("rock", &world.textures.rock),
            self.upload(
                "transitionMap",
                size,
                size,
                wgpu::TextureFormat::Bgra8Unorm,
                &[surface],
            ),
        ];
        let sampler = |mode| {
            self.device.create_sampler(&wgpu::SamplerDescriptor {
                address_mode_u: mode,
                address_mode_v: mode,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::MipmapFilterMode::Linear,
                anisotropy_clamp: 8,
                ..Default::default()
            })
        };
        let wrap = sampler(wgpu::AddressMode::Repeat);
        let clamp = sampler(wgpu::AddressMode::ClampToEdge);
        let mut entries: Vec<wgpu::BindGroupEntry> = views
            .iter()
            .enumerate()
            .map(|(i, v)| wgpu::BindGroupEntry {
                binding: i as u32,
                resource: wgpu::BindingResource::TextureView(v),
            })
            .collect();
        entries.push(wgpu::BindGroupEntry {
            binding: 5,
            resource: wgpu::BindingResource::Sampler(&wrap),
        });
        entries.push(wgpu::BindGroupEntry {
            binding: 6,
            resource: wgpu::BindingResource::Sampler(&clamp),
        });
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("terrain material"),
            layout: &self.material_layout,
            entries: &entries,
        });
        let mut indices: Vec<u8> = world
            .mesh
            .indices
            .iter()
            .flat_map(|i| i.to_le_bytes())
            .collect();
        // Index buffers must be a multiple of 4 bytes.
        indices.resize(indices.len().next_multiple_of(4), 0);
        self.terrain = Some(TerrainGpu {
            vertices: self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("terrain vertices"),
                    contents: &world.mesh.vertex_bytes(),
                    usage: wgpu::BufferUsages::VERTEX,
                }),
            indices: self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("terrain indices"),
                    contents: &indices,
                    usage: wgpu::BufferUsages::INDEX,
                }),
            index_count: world.mesh.indices.len() as u32,
            tiles: (world.mesh.tiles_per_side * world.mesh.tiles_per_side) as u32,
            bind_group,
        });
    }

    pub fn render(&mut self, frame: &Frame) {
        let Some((surface, config)) = &self.surface else {
            return;
        };
        let texture = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            other => {
                log::warn!("surface unavailable: {other:?}");
                if matches!(
                    other,
                    wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost
                ) {
                    surface.configure(&self.device, config);
                }
                return;
            }
        };
        let view = texture.texture.create_view(&Default::default());
        self.draw(&view, frame);
        self.queue.present(texture);
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn capture(&mut self, frame: &Frame) -> Vec<u8> {
        let (w, h) = self.size;
        let target = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("capture"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        self.draw(&target.create_view(&Default::default()), frame);
        let row = (w * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: (row * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, |r| r.expect("map readback"));
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("poll");
        let mapped = readback.slice(..).get_mapped_range().expect("mapped range");
        let mut out = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            let start = (y * row) as usize;
            out.extend_from_slice(&mapped[start..start + (w * 4) as usize]);
        }
        out
    }

    fn draw_terrain<'p>(&'p self, pass: &mut wgpu::RenderPass<'p>, globals: &'p wgpu::BindGroup) {
        self.draw_terrain_with(pass, globals, &self.terrain_pipeline);
    }

    fn draw_terrain_with<'p>(
        &'p self,
        pass: &mut wgpu::RenderPass<'p>,
        globals: &'p wgpu::BindGroup,
        pipeline: &'p wgpu::RenderPipeline,
    ) {
        let Some(t) = &self.terrain else { return };
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, globals, &[]);
        pass.set_bind_group(1, &t.bind_group, &[]);
        pass.set_index_buffer(t.indices.slice(..), wgpu::IndexFormat::Uint16);
        // One draw per 32×32-cell tile, each with its own 33×33 vertices (as the game does).
        // Binding at an offset avoids base-vertex, which WebGL2 lacks.
        let tile_bytes = (TILE_VERTS * TILE_VERTS * 16) as u64;
        for tile in 0..t.tiles as u64 {
            pass.set_vertex_buffer(
                0,
                t.vertices.slice(tile * tile_bytes..(tile + 1) * tile_bytes),
            );
            pass.draw_indexed(0..t.index_count, 0, 0..1);
        }
    }

    fn scene_pass<'e>(
        encoder: &'e mut wgpu::CommandEncoder,
        label: &str,
        color: &'e wgpu::TextureView,
        depth: &'e wgpu::TextureView,
        clear: Option<wgpu::Color>,
    ) -> wgpu::RenderPass<'e> {
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(label),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: clear.map_or(wgpu::LoadOp::Load, wgpu::LoadOp::Clear),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: Some(wgpu::Operations {
                    load: if clear.is_some() {
                        wgpu::LoadOp::Clear(1.0)
                    } else {
                        wgpu::LoadOp::Load
                    },
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        })
    }

    fn draw(&mut self, view: &wgpu::TextureView, frame: &Frame) {
        let scene = frame.terrain.filter(|_| self.terrain.is_some());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        if let Some(scene) = &scene {
            let water = self.extras.water_level();
            let sun = self.sun_camera(scene);
            self.shadow.lookup = Some(sun.lookup());
            self.write_globals(&self.globals, scene, None);
            if let Some(level) = water {
                self.write_globals(&self.reflect_globals, scene, Some(level));
            }
            self.extras.place_sky(&self.queue, scene.eye, scene.far);
            let fog = scene.lighting.fog_color;
            let clear = wgpu::Color {
                r: fog.x as f64,
                g: fog.y as f64,
                b: fog.z as f64,
                a: 0.0,
            };
            let t = &self.targets;
            {
                // `SunLight::DrawShadowMap`: every caster's depth pass (terrain tiles included;
                // the sky's has no programs).
                let mut pass = self.shadow.begin_pass(&mut encoder);
                self.draw_terrain_with(&mut pass, &self.shadow_bind, &self.terrain_depth_pipeline);
                self.models.draw_depth(&mut pass, &self.shadow_bind);
            }
            if water.is_some() {
                let mut pass = Self::scene_pass(
                    &mut encoder,
                    "reflection",
                    &t.reflection_view,
                    &t.reflection_depth,
                    Some(clear),
                );
                self.extras.draw_sky(&mut pass, &self.reflect_bind);
                self.draw_terrain(&mut pass, &self.reflect_bind);
                self.models.draw(&mut pass, &self.reflect_bind);
            }
            {
                let mut pass =
                    Self::scene_pass(&mut encoder, "scene", &t.scene_view, &t.depth, Some(clear));
                self.extras.draw_sky(&mut pass, &self.globals_bind);
                self.draw_terrain(&mut pass, &self.globals_bind);
                self.models.draw(&mut pass, &self.globals_bind);
                self.extras.draw_decals(&mut pass, &self.globals_bind);
            }
            if water.is_some() {
                encoder.copy_texture_to_texture(
                    t.scene.as_image_copy(),
                    t.frame_copy.as_image_copy(),
                    wgpu::Extent3d {
                        width: self.size.0,
                        height: self.size.1,
                        depth_or_array_layers: 1,
                    },
                );
                let mut pass =
                    Self::scene_pass(&mut encoder, "water", &t.scene_view, &t.depth, None);
                self.extras.draw_water(&mut pass, &self.globals_bind);
            }
            self.highlight.draw(
                &mut encoder,
                &self.queue,
                &t.scene_view,
                &t.depth,
                scene,
                self.size.0,
            );
            self.status_icons.prepare(
                &self.device,
                &self.queue,
                &self.pending_icons,
                &self.icon_sheets,
                scene,
                crate::status_icons::IconOptions::default(),
            );
            self.status_icons
                .draw(&mut encoder, &t.scene_view, &t.depth);
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("screen"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(frame.clear),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.targets.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if scene.is_some() {
                pass.set_pipeline(&self.blit_pipeline);
                pass.set_bind_group(0, &self.targets.blit_bind, &[]);
                pass.draw(0..3, 0..1);
            }
            if let Some((ui, quads)) = frame.ui {
                self.ui
                    .draw(&self.device, &self.queue, &mut pass, ui, quads, self.size);
            }
        }
        self.queue.submit([encoder.finish()]);
    }

    /// Fit the sun camera to this frame's camera (`SunLight::GetSunCamera`) and write the shadow
    /// pass's globals: the sun's view-projection and axes (billboards face the sun there).
    fn sun_camera(&self, scene: &SceneFrame) -> crate::shadow::SunCamera {
        let camera = crate::shadow::MainCamera {
            eye: scene.eye,
            right: scene.right,
            forward: scene.forward,
            up: scene.up,
            hfov: scene.hfov,
            vfov: scene.vfov,
            near: scene.near,
        };
        let sun = crate::shadow::sun_camera(&camera, scene.lighting.light_dir, self.shadow.size);
        let frame = SceneFrame {
            view_proj: sun.depth_view_proj(self.shadow.size),
            eye: sun.position,
            far: crate::shadow::SUN_FAR,
            right: sun.right,
            forward: sun.forward,
            up: sun.up,
            ..*scene
        };
        self.write_globals(&self.shadow_globals, &frame, None);
        sun
    }

    /// Write a pass's globals. With `reflect`, mirror the scene about that water height and clip
    /// what lies below it.
    fn write_globals(&self, buffer: &wgpu::Buffer, scene: &SceneFrame, reflect: Option<f32>) {
        let l = &scene.lighting;
        let (view_proj, clip) = match reflect {
            Some(level) => (
                scene.view_proj
                    * Mat4::from_translation(Vec3::new(0.0, 0.0, 2.0 * level))
                    * Mat4::from_scale(Vec3::new(1.0, 1.0, -1.0)),
                [1.0, level, 0.0, 0.0],
            ),
            None => (scene.view_proj, [0.0; 4]),
        };
        let globals = Globals {
            view_proj: view_proj.to_cols_array_2d(),
            light_dir: l.light_dir.extend(0.0).to_array(),
            light_color: l.light_color.extend(1.0).to_array(),
            ambient_color: l.ambient_color.extend(1.0).to_array(),
            fog_color: l.fog_color.extend(l.fog_max).to_array(),
            fog: Vec4::new(-l.fog_start, 1.0 / (l.fog_end - l.fog_start), 0.0, 0.0).to_array(),
            user: l.user.to_array(),
            camera_right: scene.right.extend(0.0).to_array(),
            camera_forward: scene.forward.extend(scene.time).to_array(),
            camera_up: scene.up.extend(0.0).to_array(),
            camera_pos: scene.eye.extend(1.0).to_array(),
            clip,
            shadow: self.shadow.lookup.unwrap_or(Mat4::ZERO).to_cols_array_2d(),
            shadow_params: [
                1.0 / self.shadow.size as f32,
                if self.shadow.lookup.is_some() {
                    1.0
                } else {
                    0.0
                },
                0.0,
                0.0,
            ],
        };
        self.queue
            .write_buffer(buffer, 0, bytemuck::bytes_of(&globals));
    }
}
