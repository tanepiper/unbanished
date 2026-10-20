//! The model renderer's pipelines, layouts and samplers, made once (`ModelRenderer::new`), and its
//! shader (`models.wgsl`).

use super::*;

pub(super) const SHADER: &str = include_str!("models.wgsl");

impl ModelRenderer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        globals_layout: &wgpu::BindGroupLayout,
    ) -> ModelRenderer {
        let (format, depth) = (
            crate::gfx::scene_gfx::SCENE_FORMAT,
            crate::gfx::scene_gfx::DEPTH_FORMAT,
        );
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("models"),
            source: wgpu::ShaderSource::Wgsl(
                format!(
                    "{}{}{SHADER}",
                    crate::gfx::scene_gfx::GLOBALS_WGSL,
                    banished_gfx::skin::SKIN_WGSL
                )
                .into(),
            ),
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
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                tex(4),
                tex(5),
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
                        write_mask: wgpu::ColorWrites::ALL,
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
                multisample: crate::gfx::scene_gfx::multisample(),
                multiview_mask: None,
                cache: None,
            })
        };
        let skin_layout = banished_gfx::skin::skin_layout(device, true);
        let skinned_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("skinned models"),
            bind_group_layouts: &[Some(globals_layout), Some(&layout), Some(&skin_layout)],
            immediate_size: 0,
        });
        let skin_attributes = banished_gfx::skin::skinned_attributes();
        let make_skinned = |vs: &str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(vs),
                layout: Some(&skinned_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vs),
                    compilation_options: Default::default(),
                    buffers: &[
                        Some(wgpu::VertexBufferLayout {
                            array_stride: banished_formats::types::SKINNED_STRIDE as u64,
                            step_mode: wgpu::VertexStepMode::Vertex,
                            attributes: &skin_attributes,
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
                    entry_point: Some("fs_opaque"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
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
                multisample: crate::gfx::scene_gfx::multisample(),
                multiview_mask: None,
                cache: None,
            })
        };
        let skinned_pipelines = [
            make_skinned("vs_skinned"),
            make_skinned("vs_skinned_character"),
        ];
        // Shadow pass: the colour passes' vertex programs under the sun camera's globals; only
        // the alpha-tested materials have a depth pixel shader.
        let static_attributes = wgpu::vertex_attr_array![
            0 => Sint16x4, 1 => Sint16x4, 2 => Unorm8x4, 3 => Sint16x2
        ];
        let instance_attributes = wgpu::vertex_attr_array![
            5 => Float32x4, 6 => Float32x4, 7 => Float32x4, 8 => Float32x4
        ];
        let instance = wgpu::VertexBufferLayout {
            array_stride: 64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &instance_attributes,
        };
        let static_buffers = [
            Some(wgpu::VertexBufferLayout {
                array_stride: STATIC_STRIDE as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &static_attributes,
            }),
            Some(instance.clone()),
        ];
        let skinned_buffers = [
            Some(wgpu::VertexBufferLayout {
                array_stride: banished_formats::types::SKINNED_STRIDE as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &skin_attributes,
            }),
            Some(instance),
        ];
        use crate::gfx::shadow::DepthState;
        let depth = |vs: &str, fs, state| {
            crate::gfx::shadow::depth_pipeline(
                device,
                vs,
                &pipeline_layout,
                &shader,
                vs,
                fs,
                &static_buffers,
                state,
            )
        };
        // Same order as `KINDS`: Opaque and OpaqueSnow use Depth_ps (no alpha test).
        let depth_pipelines = [
            depth("vs_main", None, DepthState::BackFaces),
            depth("vs_main", None, DepthState::BackFaces),
            depth(
                "vs_main",
                Some("fs_depth_alpha"),
                DepthState::FrontFacesBiased,
            ),
            depth(
                "vs_foliage",
                Some("fs_depth_alpha"),
                DepthState::FrontFacesBiased,
            ),
            depth(
                "vs_billboard",
                Some("fs_depth_billboard"),
                DepthState::TwoSided,
            ),
            depth("vs_character", None, DepthState::BackFaces),
        ];
        let skinned_depth = |vs: &str| {
            crate::gfx::shadow::depth_pipeline(
                device,
                vs,
                &skinned_layout,
                &shader,
                vs,
                None,
                &skinned_buffers,
                DepthState::BackFaces,
            )
        };
        let skinned_depth_pipelines = [
            skinned_depth("vs_skinned"),
            skinned_depth("vs_skinned_character"),
        ];
        // Same order as `KINDS`.
        let pipelines = [
            make("vs_main", "fs_opaque"),
            make("vs_main", "fs_snow"),
            make("vs_main", "fs_alpha"),
            make("vs_foliage", "fs_foliage"),
            make("vs_billboard", "fs_billboard"),
            make("vs_character", "fs_opaque"),
        ];
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            ..crate::gfx::scene_gfx::material_sampler(wgpu::AddressMode::Repeat)
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
            skinned_pipelines,
            depth_pipelines,
            skinned_depth_pipelines,
            skin_layout,
            layout,
            sampler,
            white,
            groups: Vec::new(),
            pass_buffers: [None, None, None],
            packed: Vec::new(),
            views: HashMap::new(),
        }
    }
}
