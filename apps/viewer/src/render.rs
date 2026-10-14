//! wgpu renderer: one pipeline per vertex stride, RGBA8 textures decoded on the CPU.

use std::sync::Arc;

use banished_formats::skeleton::{bind_palette, local_bind, world_transforms, Transform};
use banished_formats::texture::decode_rgba;
use banished_formats::types::{AnimationClip, Bone, ImageBuffer, SKINNED_STRIDE, STATIC_STRIDE};
use glam::{Mat4, Vec3};
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

const MAX_BONES: usize = 32;

struct GpuPart {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
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
    globals: wgpu::Buffer,
    globals_group: wgpu::BindGroup,
    texture_layout: wgpu::BindGroupLayout,
    skin_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    white: wgpu::TextureView,
    pub backend: String,
    pub ui: crate::ui_render::UiRenderer,
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
            source: wgpu::ShaderSource::Wgsl(include_str!("mesh.wgsl").into()),
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
        let skin_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("skin"),
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
        let skinned_attributes = [
            static_attributes[0],
            static_attributes[1],
            static_attributes[2],
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Unorm8x4,
                offset: 24,
                shader_location: 3,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Uint8x4,
                offset: 28,
                shader_location: 4,
            },
        ];
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
        let ui = crate::ui_render::UiRenderer::new(&device, format, DEPTH_FORMAT);

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

    /// Upload a referenced image; also returns its aspect (height / width).
    fn texture(
        &self,
        assets: &Assets,
        reference: Option<&str>,
    ) -> Option<(wgpu::TextureView, f32)> {
        let reference = reference?;
        let img = match assets.image(reference) {
            Ok(img) => img,
            Err(e) => {
                log::warn!("{reference}: {e}");
                return None;
            }
        };
        let aspect = img.height as f32 / img.width.max(1) as f32;
        upload_image(&self.device, &self.queue, reference, &img).map(|v| (v, aspect))
    }

    pub fn load_mesh(&self, assets: &Assets, name: &str) -> banished_formats::Result<GpuMesh> {
        let mesh = assets.mesh(name)?;
        let mut parts = Vec::new();
        let locals: Vec<_> = mesh.bones.iter().map(local_bind).collect();
        let world = world_transforms(&mesh.bones, &locals);
        // Parts are detail levels; the asset browser displays the most detailed one.
        for part in mesh.parts.iter().take(1) {
            for range in &part.ranges {
                let (index_data, vertex_data, _) = part.geometry(range);
                if range.index_count == 0 || index_data.is_empty() {
                    continue;
                }
                let instance_ref = mesh
                    .material_instances
                    .iter()
                    .find_map(|set| set.get(range.material_index as usize).cloned().flatten());
                let instance = instance_ref.as_deref().and_then(|r| {
                    assets
                        .material_instance(r)
                        .map_err(|e| log::warn!("{r}: {e}"))
                        .ok()
                });
                let diffuse_ref = instance
                    .as_ref()
                    .and_then(|mi| mi.texture("diffuse").map(str::to_owned));
                let (diffuse, aspect) = self
                    .texture(assets, diffuse_ref.as_deref())
                    .unwrap_or_else(|| (self.white.clone(), 1.0));
                let ao = self
                    .texture(assets, part.ao_texture.as_deref())
                    .map_or_else(|| self.white.clone(), |t| t.0);
                // Character materials index one square cell of a horizontal outfit atlas (variant 0 here).
                let character = instance
                    .as_ref()
                    .and_then(|mi| mi.material.as_deref())
                    .is_some_and(|m| m.contains("OpaqueCharacterMaterial"));
                let uv_transform: [f32; 4] = if character {
                    [aspect, 1.0, 0.0, 0.0]
                } else {
                    [1.0, 1.0, 0.0, 0.0]
                };
                // Rigid triangle lists use the node's world transform; weighted lists already
                // apply world * inverse_bind through the skin palette in the vertex shader.
                let rigid = world
                    .get(range.bone as usize)
                    .copied()
                    .unwrap_or(Transform::IDENTITY);
                let mut material_params = uv_transform.to_vec();
                material_params.extend_from_slice(&rigid.to_matrix());
                let params = self
                    .device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("material params"),
                        contents: bytemuck::cast_slice(&material_params),
                        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    });
                let textures = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some(&part.name),
                    layout: &self.texture_layout,
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
                            resource: params.as_entire_binding(),
                        },
                    ],
                });
                let buffer = |data: &[u8], usage| {
                    self.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some(&part.name),
                            contents: data,
                            usage,
                        })
                };
                // Index buffers must be 4-byte aligned in size for writes on some backends.
                let mut indices = index_data.to_vec();
                indices.resize(indices.len().next_multiple_of(4), 0);
                parts.push(GpuPart {
                    vertices: buffer(vertex_data, wgpu::BufferUsages::VERTEX),
                    indices: buffer(&indices, wgpu::BufferUsages::INDEX),
                    index_range: range.start_index as u32
                        ..(range.start_index + range.index_count) as u32,
                    base_vertex: range.start_vertex,
                    skinned: range.bone < 0,
                    rigid_bone: usize::try_from(range.bone).ok(),
                    textures,
                    material_params: params,
                });
            }
        }
        let mut palette = bind_palette(&mesh.bones);
        if palette.len() > MAX_BONES {
            log::warn!(
                "{name}: {} bones exceeds palette size {MAX_BONES}",
                palette.len()
            );
        }
        palette.resize(MAX_BONES, Transform::IDENTITY);
        let matrices: Vec<[f32; 16]> = palette.iter().map(Transform::to_matrix).collect();
        let skin_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("skin"),
                contents: bytemuck::cast_slice(&matrices),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
        let skin = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("skin"),
            layout: &self.skin_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: skin_buffer.as_entire_binding(),
            }],
        });
        let (min, max) = (Vec3::from(mesh.bounds_min), Vec3::from(mesh.bounds_max));
        Ok(GpuMesh {
            parts,
            skin,
            skin_buffer,
            bones: mesh.bones,
            center: (min + max) * 0.5,
            radius: ((max - min).length() * 0.5).max(0.1),
        })
    }

    pub fn set_animation_pose(&self, mesh: &GpuMesh, clip: &AnimationClip, seconds: f32) {
        if clip.frames.is_empty()
            || clip.bone_count != mesh.bones.len()
            || mesh.bones.len() > MAX_BONES
            || clip.sample_rate <= 0.0
        {
            return;
        }
        let frame_position = (seconds.max(0.0) * clip.sample_rate).max(0.0);
        let count = clip.frames.len();
        let raw_frame = frame_position.floor() as usize;
        let first = if raw_frame < count {
            raw_frame
        } else if clip.looping {
            let loop_start = clip.loop_start.min(count - 1);
            loop_start + raw_frame % (count - loop_start).max(1)
        } else {
            count - 1
        };
        let second = if first + 1 < count {
            first + 1
        } else if clip.looping {
            clip.loop_start.min(count - 1)
        } else {
            first
        };
        let t = frame_position.fract();
        let locals: Vec<_> = clip.frames[first]
            .iter()
            .zip(&clip.frames[second])
            .map(|(a, b)| {
                let mut qb = glam::Quat::from_xyzw(
                    b.rotation[0],
                    b.rotation[1],
                    b.rotation[2],
                    b.rotation[3],
                );
                let qa = glam::Quat::from_xyzw(
                    a.rotation[0],
                    a.rotation[1],
                    a.rotation[2],
                    a.rotation[3],
                );
                if qa.dot(qb) < 0.0 {
                    qb = -qb;
                }
                let q = qa.lerp(qb, t).normalize();
                Transform {
                    rotation: q.to_array(),
                    translation: std::array::from_fn(|i| {
                        a.translation[i] + (b.translation[i] - a.translation[i]) * t
                    }),
                }
            })
            .collect();
        let world = world_transforms(&mesh.bones, &locals);
        for part in &mesh.parts {
            if let Some(bone) = part.rigid_bone.and_then(|i| world.get(i)) {
                self.queue.write_buffer(
                    &part.material_params,
                    16,
                    bytemuck::cast_slice(&bone.to_matrix()),
                );
            }
        }
        let palette = banished_formats::skeleton::palette(&mesh.bones, &locals);
        let matrices: Vec<[f32; 16]> = palette.iter().map(Transform::to_matrix).collect();
        self.queue
            .write_buffer(&mesh.skin_buffer, 0, bytemuck::cast_slice(&matrices));
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

    /// Render one frame offscreen and return it as tightly packed RGBA8.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn capture(&mut self, frame: Frame) -> Vec<u8> {
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

    fn draw(&mut self, view: &wgpu::TextureView, frame: Frame) {
        let light = Vec3::new(-0.4, -0.3, -1.0).normalize();
        let globals = Globals {
            view_proj: frame.view_proj.to_cols_array_2d(),
            light_dir: light
                .extend(if frame.vertices_only { 1.0 } else { 0.0 })
                .to_array(),
            eye: frame
                .eye
                .extend(if frame.textures { 1.0 } else { 0.0 })
                .to_array(),
        };
        self.queue
            .write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));

        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main"),
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
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if let Some(mesh) = frame.mesh {
                let [x, y, w, h] =
                    frame
                        .viewport
                        .unwrap_or([0.0, 0.0, self.size.0 as f32, self.size.1 as f32]);
                let (x, y) = (x.max(0.0), y.max(0.0));
                let (w, h) = (w.min(self.size.0 as f32 - x), h.min(self.size.1 as f32 - y));
                if w >= 1.0 && h >= 1.0 {
                    pass.set_viewport(x, y, w, h, 0.0, 1.0);
                    pass.set_bind_group(0, &self.globals_group, &[]);
                    pass.set_bind_group(2, &mesh.skin, &[]);
                    for part in &mesh.parts {
                        pass.set_pipeline(if frame.vertices_only {
                            if part.skinned {
                                &self.skinned_points
                            } else {
                                &self.static_points
                            }
                        } else if part.skinned {
                            &self.skinned_pipeline
                        } else {
                            &self.static_pipeline
                        });
                        pass.set_bind_group(1, &part.textures, &[]);
                        pass.set_vertex_buffer(0, part.vertices.slice(..));
                        pass.set_index_buffer(part.indices.slice(..), wgpu::IndexFormat::Uint16);
                        pass.draw_indexed(part.index_range.clone(), part.base_vertex, 0..1);
                    }
                }
            }
            if let Some((ui, quads)) = frame.ui {
                self.ui
                    .draw(&self.device, &self.queue, &mut pass, ui, quads, self.size);
            }
        }
        self.queue.submit([encoder.finish()]);
    }
}

/// Everything drawn in one frame.
pub struct Frame<'a> {
    pub clear: wgpu::Color,
    pub mesh: Option<&'a GpuMesh>,
    pub view_proj: Mat4,
    pub eye: Vec3,
    /// Pixel rectangle (x, y, w, h) for the 3D view; the whole target when `None`.
    pub viewport: Option<[f32; 4]>,
    pub ui: Option<(&'a banished_ui::Ui, &'a [banished_ui::Quad])>,
    pub textures: bool,
    pub vertices_only: bool,
}

pub const CLEAR_SKY: wgpu::Color = wgpu::Color {
    r: 0.42,
    g: 0.52,
    b: 0.62,
    a: 1.0,
};
pub const CLEAR_DARK: wgpu::Color = wgpu::Color {
    r: 0.075,
    g: 0.08,
    b: 0.09,
    a: 1.0,
};

pub(crate) fn upload_rgba(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    width: u32,
    height: u32,
    mips: &[Vec<u8>],
) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: mips.len() as u32,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (level, data) in mips.iter().enumerate() {
        let (w, h) = ((width >> level).max(1), (height >> level).max(1));
        queue.write_texture(
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

pub(crate) fn upload_image(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    img: &ImageBuffer,
) -> Option<wgpu::TextureView> {
    let mips: Option<Vec<Vec<u8>>> = img.levels[..img.mip_count()]
        .iter()
        .enumerate()
        .map(|(i, level)| decode_rgba(img.format, img.width >> i, img.height >> i, level))
        .collect();
    let mips = mips?;
    if mips.is_empty() {
        return None;
    }
    Some(upload_rgba(
        device, queue, label, img.width, img.height, &mips,
    ))
}
