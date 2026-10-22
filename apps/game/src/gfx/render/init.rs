//! Creating the renderer: the device, the surface, the pipelines and the targets.

use super::*;

impl Renderer {
    pub(super) async fn init(
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
                format!("{GLOBALS_WGSL}{}", include_str!("../terrain.wgsl")).into(),
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
        let mut ui = unbanished_gfx::UiRenderer::new(&device, format, DEPTH_FORMAT);
        let white = unbanished_gfx::upload_rgba_as(
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
}
