//! 3D content drawn behind a loop's UI.

use glam::{Mat4, Vec2, Vec3};

use crate::render::Lighting;
use crate::world::World;

/// A terrain world with its camera and lighting. `id` changes whenever `world` does, so the
/// renderer knows when to upload it again.
pub struct SceneView {
    pub id: u64,
    pub world: World,
    pub camera: Camera,
    pub lighting: Lighting,
}

static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

impl SceneView {
    pub fn new(world: World, camera: Camera, lighting: Lighting) -> SceneView {
        SceneView {
            id: NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            world,
            camera,
            lighting,
        }
    }

    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        self.camera.view_proj(&self.world, aspect)
    }
}

/// Strategy camera over the terrain (z up), orbiting a ground target.
#[derive(Clone, Copy)]
pub struct Camera {
    pub target: Vec2,
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
}

impl Camera {
    pub fn overview(world: &World) -> Camera {
        Camera {
            target: Vec2::ZERO,
            yaw: 0.0,
            pitch: 60f32.to_radians(),
            distance: world.size as f32 * 1.1,
        }
    }

    pub fn ground() -> Camera {
        Camera {
            target: Vec2::ZERO,
            yaw: 0.5,
            pitch: 40f32.to_radians(),
            distance: 60.0,
        }
    }

    fn focus(&self, world: &World) -> Vec3 {
        self.target
            .extend(world.height_at(self.target.x, self.target.y).max(0.0))
    }

    fn eye(&self, world: &World) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        self.focus(world) + Vec3::new(-sy * cp, -cy * cp, sp) * self.distance
    }

    pub fn view_proj(&self, world: &World, aspect: f32) -> Mat4 {
        use glam::camera::rh::{proj::directx::perspective, view::look_at_mat4};
        let proj = perspective(45f32.to_radians(), aspect, 0.5, self.distance * 4.0 + 100.0);
        proj * look_at_mat4(self.eye(world), self.focus(world), Vec3::Z)
    }

    pub fn pan(&mut self, forward: f32, right: f32, world: &World) {
        let (sy, cy) = self.yaw.sin_cos();
        let step = self.distance * 0.02;
        self.target += Vec2::new(sy, cy) * forward * step + Vec2::new(cy, -sy) * right * step;
        let half = world.size as f32 / 2.0;
        self.target = self.target.clamp(Vec2::splat(-half), Vec2::splat(half));
    }

    pub fn rotate(&mut self, dx: f32, dy: f32) {
        self.yaw += dx * 0.005;
        self.pitch = (self.pitch + dy * 0.005).clamp(10f32.to_radians(), 89f32.to_radians());
    }

    pub fn zoom(&mut self, steps: f32) {
        self.distance = (self.distance * 0.9f32.powf(steps)).clamp(5.0, 2000.0);
    }
}
