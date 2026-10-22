//! Bind group entries, samplers, the sample count and the scene pipelines.

use super::*;

pub(super) fn tex_entry(
    binding: u32,
    dim: wgpu::TextureViewDimension,
) -> wgpu::BindGroupLayoutEntry {
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

pub(super) fn sampler_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
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
pub(super) static TEXTURE_FILTER: std::sync::atomic::AtomicI32 =
    std::sync::atomic::AtomicI32::new(0);

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

pub(super) struct PipelineSpec<'a> {
    pub(super) label: &'a str,
    pub(super) vs: &'a str,
    pub(super) fs: &'a str,
    pub(super) stride: u64,
    pub(super) attributes: &'a [wgpu::VertexAttribute],
    pub(super) blend: Option<wgpu::BlendState>,
    pub(super) depth_write: bool,
    pub(super) depth_compare: wgpu::CompareFunction,
    pub(super) bias: wgpu::DepthBiasState,
}

/// The scene's sample count (antialiasing, `Video::Configuration +0x24`), set when the
/// renderer starts: every pipeline drawing into the scene or the reflection is made with it.
pub(super) static SAMPLES: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

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

pub(super) fn pipeline(
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

pub(super) fn linear_view(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    img: &ImageBuffer,
    fallback: &wgpu::TextureView,
) -> wgpu::TextureView {
    unbanished_gfx::upload_image_linear(device, queue, "scene texture", img)
        .unwrap_or_else(|| fallback.clone())
}
