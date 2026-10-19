//! Video settings: backends, present mode, and applying a changed configuration.

use super::*;

/// The wgpu backends in the order the Renderer combo lists them.
pub(super) const BACKENDS: [(wgpu::Backend, &str); 5] = [
    (wgpu::Backend::Vulkan, "Vulkan"),
    (wgpu::Backend::Metal, "Metal"),
    (wgpu::Backend::Dx12, "DirectX 12"),
    (wgpu::Backend::Gl, "OpenGL"),
    (wgpu::Backend::BrowserWebGpu, "WebGPU"),
];

pub(super) fn present_mode(vsync: bool) -> wgpu::PresentMode {
    if vsync {
        wgpu::PresentMode::AutoVsync
    } else {
        wgpu::PresentMode::AutoNoVsync
    }
}

impl Renderer {
    /// `Video::Device::ApplyChangedSettings`: the settings that need no new renderer.
    pub fn apply_video(&mut self, video: VideoSettings) {
        if let Some((surface, config)) = &mut self.surface {
            let mode = present_mode(video.vsync);
            if config.present_mode != mode {
                config.present_mode = mode;
                surface.configure(&self.device, config);
            }
        }
        self.video = video;
    }
}

impl Default for VideoSettings {
    fn default() -> Self {
        Self::from(&crate::settings::VideoConfig::default())
    }
}

impl From<&crate::settings::VideoConfig> for VideoSettings {
    fn from(v: &crate::settings::VideoConfig) -> Self {
        VideoSettings {
            interface: v.interface,
            adapter: v.adapter,
            filter: v.texture_filter,
            shadow_scalar: v.shadow_scalar,
            shadow_detail: v.shader_detail,
            reflection: v.reflection_detail,
            vsync: v.vsync,
            multisample: v.multisample,
        }
    }
}

impl VideoSettings {
    /// Whether going from `self` to `next` needs a new renderer (`Video::Device::Shutdown` /
    /// `Startup` in the original for the interface and adapter; here also the samplers and the
    /// shadow map, which are made at creation).
    pub fn needs_restart(&self, next: &VideoSettings) -> bool {
        self.interface != next.interface
            || self.adapter != next.adapter
            || self.filter != next.filter
            || self.shadow_scalar != next.shadow_scalar
            || self.multisample != next.multisample
    }
}
