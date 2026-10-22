//! The window's and the scene's sizes, and resizing the targets.

use super::*;

impl Renderer {
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

    pub(super) fn resize_scene(&mut self) {
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

    pub(super) fn targets_size(&self) -> (u32, u32) {
        let s = self.targets.scene.size();
        (s.width, s.height)
    }

    /// Whether the window's size differs from the screen depth's (made with the targets).
    pub(super) fn screen_resized(&self) -> bool {
        let s = self.targets.screen_depth.texture().size();
        (s.width, s.height) != self.size
    }

    pub fn aspect(&self) -> f32 {
        self.size.0 as f32 / self.size.1 as f32
    }
}
