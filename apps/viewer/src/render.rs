//! wgpu renderer: one pipeline per vertex stride, RGBA8 textures decoded on the CPU.

mod draw;
mod mesh;
pub use draw::{Frame, RenderMode, CLEAR_DARK, CLEAR_SKY};

use std::sync::Arc;

use glam::{Mat4, Vec3};
use unbanished_formats::skeleton::{bind_palette, world_transforms};
use unbanished_formats::types::{AnimationClip, Bone, SKINNED_STRIDE, STATIC_STRIDE};
use unbanished_gfx::material::{character_uv_transform, MaterialKind};
use unbanished_gfx::{upload_image, upload_rgba};
use wgpu::util::DeviceExt;
use winit::window::Window;

use crate::assets::Assets;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    light_dir: [f32; 4],
    eye: [f32; 4],
}

use unbanished_gfx::skin::MAX_BONES;

struct GpuPart {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    wire_indices: wgpu::Buffer,
    wire_index_count: u32,
    index_range: std::ops::Range<u32>,
    base_vertex: i32,
    skinned: bool,
    rigid_bone: Option<usize>,
    textures: wgpu::BindGroup,
    material_params: wgpu::Buffer,
}

impl GpuMesh {
    pub fn part_count(&self) -> usize {
        self.parts.len()
    }
}

pub struct GpuMesh {
    parts: Vec<GpuPart>,
    skin: wgpu::BindGroup,
    skin_buffer: wgpu::Buffer,
    bones: Vec<Bone>,
    pub center: Vec3,
    pub radius: f32,
}

pub struct Renderer {
    surface: Option<(wgpu::Surface<'static>, wgpu::SurfaceConfiguration)>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    format: wgpu::TextureFormat,
    size: (u32, u32),
    depth: wgpu::TextureView,
    static_pipeline: wgpu::RenderPipeline,
    skinned_pipeline: wgpu::RenderPipeline,
    static_points: wgpu::RenderPipeline,
    skinned_points: wgpu::RenderPipeline,
    static_wireframe: wgpu::RenderPipeline,
    skinned_wireframe: wgpu::RenderPipeline,
    globals: wgpu::Buffer,
    globals_group: wgpu::BindGroup,
    texture_layout: wgpu::BindGroupLayout,
    skin_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    white: wgpu::TextureView,
    pub backend: String,
    pub ui: unbanished_gfx::UiRenderer,
}

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

fn depth_view(device: &wgpu::Device, w: u32, h: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d {
                width: w.max(1),
                height: h.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&Default::default())
}

impl Renderer {
    /// Renderer presenting to a window.
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

    /// Offscreen renderer for `--capture`.
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
                label: Some("viewer"),
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
            if let Some(srgb) = caps.formats.iter().find(|f| f.is_srgb()) {
                config.format = *srgb;
            }
            surface.configure(&device, &config);
            (surface, config)
        });
        let format = surface
            .as_ref()
            .map_or(wgpu::TextureFormat::Rgba8UnormSrgb, |(_, c)| c.format);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mesh"),
            source: wgpu::ShaderSource::Wgsl(
                format!(
                    "{}{}",
                    unbanished_gfx::skin::SKIN_WGSL,
                    include_str!("mesh.wgsl")
                )
                .into(),
            ),
        });

        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals"),
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
        let tex_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("material"),
            entries: &[
                tex_entry(0),
                tex_entry(1),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let skin_layout = unbanished_gfx::skin::skin_layout(&device, false);
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mesh"),
            bind_group_layouts: &[
                Some(&globals_layout),
                Some(&texture_layout),
                Some(&skin_layout),
            ],
            immediate_size: 0,
        });

        let static_attributes = [
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Sint16x4,
                offset: 0,
                shader_location: 0,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Sint16x4,
                offset: 8,
                shader_location: 1,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Unorm8x4,
                offset: 16,
                shader_location: 2,
            },
        ];
        let skinned_attributes = unbanished_gfx::skin::skinned_attributes();
        let pipeline = |stride: usize,
                        entry: &str,
                        attributes: &[wgpu::VertexAttribute],
                        label: &str,
                        topology: wgpu::PrimitiveTopology| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: stride as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes,
                    })],
                },
                primitive: wgpu::PrimitiveState {
                    // The original renderer culls with D3D's clockwise front faces.
                    front_face: wgpu::FrontFace::Cw,
                    cull_mode: None,
                    topology,
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
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(format.into())],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let static_pipeline = pipeline(
            STATIC_STRIDE,
            "vs_main",
            &static_attributes,
            "static",
            wgpu::PrimitiveTopology::TriangleList,
        );
        let skinned_pipeline = pipeline(
            SKINNED_STRIDE,
            "vs_skinned",
            &skinned_attributes,
            "skinned",
            wgpu::PrimitiveTopology::TriangleList,
        );
        let static_points = pipeline(
            STATIC_STRIDE,
            "vs_main",
            &static_attributes,
            "static points",
            wgpu::PrimitiveTopology::PointList,
        );
        let skinned_points = pipeline(
            SKINNED_STRIDE,
            "vs_skinned",
            &skinned_attributes,
            "skinned points",
            wgpu::PrimitiveTopology::PointList,
        );
        let static_wireframe = pipeline(
            STATIC_STRIDE,
            "vs_main",
            &static_attributes,
            "static wireframe",
            wgpu::PrimitiveTopology::LineList,
        );
        let skinned_wireframe = pipeline(
            SKINNED_STRIDE,
            "vs_skinned",
            &skinned_attributes,
            "skinned wireframe",
            wgpu::PrimitiveTopology::LineList,
        );

        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let globals_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals"),
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals.as_entire_binding(),
            }],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let white = upload_rgba(&device, &queue, "white", 1, 1, &[vec![255; 4]]);
        let depth = depth_view(&device, width, height);
        let ui = unbanished_gfx::UiRenderer::new(&device, format, DEPTH_FORMAT);

        Renderer {
            surface,
            device,
            queue,
            format,
            size: (width, height),
            depth,
            static_pipeline,
            skinned_pipeline,
            static_points,
            skinned_points,
            static_wireframe,
            skinned_wireframe,
            globals,
            globals_group,
            texture_layout,
            skin_layout,
            sampler,
            white,
            backend,
            ui,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        log::debug!("resize {width}x{height}");
        self.size = (width, height);
        if let Some((surface, config)) = &mut self.surface {
            config.width = width;
            config.height = height;
            surface.configure(&self.device, config);
        }
        self.depth = depth_view(&self.device, width, height);
    }

    pub fn aspect(&self) -> f32 {
        self.size.0 as f32 / self.size.1.max(1) as f32
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    /// Register an application texture the UI can draw as `Texture::External(id)`.
    pub fn set_ui_texture(&mut self, id: u32, view: &wgpu::TextureView) {
        self.ui.set_external(&self.device, id, view);
    }

    pub fn render(&mut self, frame: Frame) {
        let Some((surface, config)) = &self.surface else {
            return;
        };
        let target = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f)
            | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            other => {
                log::debug!("skipping frame: {other:?}");
                if matches!(
                    other,
                    wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost
                ) {
                    surface.configure(&self.device, config);
                }
                return;
            }
        };
        let view = target.texture.create_view(&Default::default());
        self.draw(&view, frame);
        self.queue.present(target);
    }
}
