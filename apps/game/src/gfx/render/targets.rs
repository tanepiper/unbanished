//! The offscreen render targets: creating them on resize and resolving antialiased passes.

use super::*;

impl Targets {
    /// `size` is the 3D scene's; `screen` the window's, for the depth of the passes drawn
    /// straight to the window (the copy of the scene, the interface, the minimap).
    pub(super) fn new(
        device: &wgpu::Device,
        size: (u32, u32),
        screen: (u32, u32),
        blit_layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
    ) -> Targets {
        let samples = crate::gfx::scene_gfx::samples();
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
            reflection_depth: view(&texture_samples(
                device,
                "reflection depth",
                half,
                DEPTH_FORMAT,
                U::RENDER_ATTACHMENT,
                samples,
            )),
            depth: view(&texture_samples(
                device,
                "depth",
                size,
                DEPTH_FORMAT,
                U::RENDER_ATTACHMENT,
                samples,
            )),
            scene_draw: (samples > 1).then(|| {
                view(&texture_samples(
                    device,
                    "scene (multisampled)",
                    size,
                    SCENE_FORMAT,
                    U::RENDER_ATTACHMENT,
                    samples,
                ))
            }),
            reflection_draw: (samples > 1).then(|| {
                view(&texture_samples(
                    device,
                    "reflection (multisampled)",
                    half,
                    SCENE_FORMAT,
                    U::RENDER_ATTACHMENT,
                    samples,
                ))
            }),
            screen_depth: view(&texture(
                device,
                "screen depth",
                screen,
                DEPTH_FORMAT,
                U::RENDER_ATTACHMENT,
            )),
            scene_view,
            scene,
            blit_bind,
        }
    }

    /// Where the scene is drawn: its multisampled colour, else `scene` itself.
    pub(super) fn scene_target(&self) -> &wgpu::TextureView {
        self.scene_draw.as_ref().unwrap_or(&self.scene_view)
    }

    pub(super) fn reflection_target(&self) -> &wgpu::TextureView {
        self.reflection_draw
            .as_ref()
            .unwrap_or(&self.reflection_view)
    }

    /// Resolve the multisampled scene into `scene` (nothing without antialiasing).
    pub(super) fn resolve_scene(&self, encoder: &mut wgpu::CommandEncoder) {
        if let Some(draw) = &self.scene_draw {
            resolve(encoder, draw, &self.scene_view);
        }
    }

    pub(super) fn resolve_reflection(&self, encoder: &mut wgpu::CommandEncoder) {
        if let Some(draw) = &self.reflection_draw {
            resolve(encoder, draw, &self.reflection_view);
        }
    }
}
