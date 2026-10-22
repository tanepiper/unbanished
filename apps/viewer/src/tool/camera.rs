//! The orbit camera: its eye and its view-projection matrix.

use super::*;

pub struct Camera {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub target: Vec3,
}

impl Camera {
    pub fn eye(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        self.target + Vec3::new(cy * cp, sy * cp, sp) * self.distance
    }

    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        use glam::camera::rh::{proj::directx::perspective, view::look_at_mat4};
        let proj = perspective(
            45f32.to_radians(),
            aspect,
            self.distance * 0.01,
            self.distance * 20.0,
        );
        proj * look_at_mat4(self.eye(), self.target, Vec3::Z)
    }
}
