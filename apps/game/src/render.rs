//! wgpu renderer for the game world. Native windows, the web canvas and headless captures share
//! one path. Colours are handled as raw (non-sRGB) values like the original D3D9 renderer.

use std::sync::Arc;

use banished_formats::texture::decode_rgba;
use banished_formats::types::ImageBuffer;
use banished_game::terrain::TILE_VERTS;
use glam::{Mat4, Vec3, Vec4};
use wgpu::util::DeviceExt;
use winit::window::Window;

use crate::world::Terrain;

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const CAPTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

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
    /// Camera axes in world space for billboards (`gc_worldToCamera` columns); `forward.w` is
    /// the time in seconds (`gc_time`).
    camera_right: [f32; 4],
    camera_forward: [f32; 4],
    camera_up: [f32; 4],
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
    pub right: Vec3,
    pub forward: Vec3,
    pub up: Vec3,
    pub time: f32,
}

/// Everything drawn in one frame: optional terrain and models, then UI quads.
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

pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: Option<(wgpu::Surface<'static>, wgpu::SurfaceConfiguration)>,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    format: wgpu::TextureFormat,
    size: (u32, u32),
    depth: wgpu::TextureView,
    pipeline: wgpu::RenderPipeline,
    globals: wgpu::Buffer,
    globals_bind: wgpu::BindGroup,
    material_layout: wgpu::BindGroupLayout,
    terrain: Option<TerrainGpu>,
    /// `SceneView::id` of the uploaded terrain.
    pub world_id: u64,
    ui: banished_gfx::UiRenderer,
    models: crate::models::ModelRenderer,
    pub backend: String,
}

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

fn color(argb: u32) -> Vec3 {
    let c = |s: u32| ((argb >> s) & 0xff) as f32 / 255.0;
    Vec3::new(c(16), c(8), c(0))
}

impl Lighting {
    /// One month of `EnvironmentDescription`, as `FUN_140098da0` applies it.
    pub fn from_month(m: &banished_formats::game::EnvironmentMonth) -> Lighting {
        let (yaw, pitch) = (m.sun_yaw.to_radians(), m.sun_pitch.to_radians());
        // Math::Vector3::FromYawPitch points towards the sun; the shader wants the travel
        // direction (inferred from the sign of its diffuse term).
        let to_sun = Vec3::new(
            -pitch.cos() * yaw.sin(),
            pitch.cos() * yaw.cos(),
            pitch.sin(),
        );
        Lighting {
            light_dir: -to_sun,
            light_color: color(m.sun_color),
            ambient_color: color(m.ambient_color),
            fog_color: color(m.fog_color),
            fog_max: m.fog_max,
            fog_start: m.fog_start,
            fog_end: m.fog_end,
            user: Vec4::ZERO,
        }
    }
}

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

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("terrain"),
            source: wgpu::ShaderSource::Wgsl(include_str!("terrain.wgsl").into()),
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
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("terrain"),
            bind_group_layouts: &[Some(&globals_layout), Some(&material_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("terrain"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
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
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::COLOR,
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
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let globals_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals"),
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals.as_entire_binding(),
            }],
        });
        let models = crate::models::ModelRenderer::new(
            &device,
            &queue,
            &globals_layout,
            format,
            DEPTH_FORMAT,
        );
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
        Renderer {
            depth: depth_view(&device, width, height),
            device,
            queue,
            surface,
            format,
            size: (width, height),
            pipeline,
            globals,
            globals_bind,
            material_layout,
            terrain: None,
            world_id: 0,
            ui,
            models,
            backend,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        let (width, height) = (width.max(1), height.max(1));
        self.size = (width, height);
        if let Some((surface, config)) = &mut self.surface {
            config.width = width;
            config.height = height;
            surface.configure(&self.device, config);
        }
        self.depth = depth_view(&self.device, width, height);
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

    /// Upload a scene's world if it is not the one on the GPU.
    pub fn set_scene(&mut self, view: &crate::scene::SceneView) {
        if self.world_id != view.id {
            self.set_terrain(&view.terrain);
            match &view.models {
                Some(m) => self.models.upload(&self.device, &self.queue, m),
                None => self.models.clear(),
            }
            self.world_id = view.id;
        }
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
        let Some((surface, _)) = &self.surface else {
            return;
        };
        let texture = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            other => {
                log::warn!("surface unavailable: {other:?}");
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

    fn draw(&mut self, view: &wgpu::TextureView, frame: &Frame) {
        let draw_terrain = frame.terrain.is_some() && self.terrain.is_some();
        if let Some(scene) = &frame.terrain {
            self.write_globals(scene);
        }
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("world"),
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
                    view: &self.depth,
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
            if let (true, Some(t)) = (draw_terrain, &self.terrain) {
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &self.globals_bind, &[]);
                pass.set_bind_group(1, &t.bind_group, &[]);
                pass.set_index_buffer(t.indices.slice(..), wgpu::IndexFormat::Uint16);
                // One draw per 32×32-cell tile, each with its own 33×33 vertices (as the game
                // does). Binding at an offset avoids base-vertex, which WebGL2 lacks.
                let tile_bytes = (TILE_VERTS * TILE_VERTS * 16) as u64;
                for tile in 0..t.tiles as u64 {
                    pass.set_vertex_buffer(
                        0,
                        t.vertices.slice(tile * tile_bytes..(tile + 1) * tile_bytes),
                    );
                    pass.draw_indexed(0..t.index_count, 0, 0..1);
                }
                self.models.draw(&mut pass, &self.globals_bind);
            }
            if let Some((ui, quads)) = frame.ui {
                self.ui
                    .draw(&self.device, &self.queue, &mut pass, ui, quads, self.size);
            }
        }
        self.queue.submit([encoder.finish()]);
    }

    fn write_globals(&mut self, scene: &SceneFrame) {
        let l = &scene.lighting;
        let globals = Globals {
            view_proj: scene.view_proj.to_cols_array_2d(),
            light_dir: l.light_dir.extend(0.0).to_array(),
            light_color: l.light_color.extend(1.0).to_array(),
            ambient_color: l.ambient_color.extend(1.0).to_array(),
            fog_color: l.fog_color.extend(l.fog_max).to_array(),
            fog: Vec4::new(-l.fog_start, 1.0 / (l.fog_end - l.fog_start), 0.0, 0.0).to_array(),
            user: l.user.to_array(),
            camera_right: scene.right.extend(0.0).to_array(),
            camera_forward: scene.forward.extend(scene.time).to_array(),
            camera_up: scene.up.extend(0.0).to_array(),
        };
        self.queue
            .write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));
    }
}
