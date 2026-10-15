//! 3D content drawn behind a loop's UI.

use glam::{Mat4, Vec2, Vec3, Vec4};

use crate::render::Lighting;
use crate::world::Terrain;

/// A terrain with its camera and lighting. `id` changes whenever `terrain` does, so the renderer
/// knows when to upload it again.
pub struct SceneView {
    pub id: u64,
    pub terrain: Terrain,
    pub models: Option<crate::models::SceneModels>,
    pub extras: Option<crate::scene_assets::SceneExtras>,
    pub camera: SceneCamera,
    pub lighting: Lighting,
    pub highlight: crate::highlight::Geometry,
}

pub enum SceneCamera {
    Map(Camera),
    Menu(MenuCamera),
}

static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

impl SceneView {
    pub fn new(terrain: Terrain, camera: SceneCamera, lighting: Lighting) -> SceneView {
        SceneView {
            id: NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            terrain,
            models: None,
            extras: None,
            camera,
            lighting,
            highlight: Default::default(),
        }
    }

    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        match &self.camera {
            SceneCamera::Map(c) => c.view_proj(&self.terrain, aspect),
            SceneCamera::Menu(c) => c.view_proj(aspect),
        }
    }

    /// Eye and look-at target in world space.
    pub fn eye_target(&self) -> (Vec3, Vec3) {
        match &self.camera {
            SceneCamera::Map(c) => (c.eye(&self.terrain), c.focus(&self.terrain)),
            SceneCamera::Menu(c) => (c.eye(), c.target),
        }
    }

    /// Camera, lighting and the camera's right/forward/up axes for this frame.
    pub fn frame(&self, aspect: f32, time: f32) -> crate::render::SceneFrame {
        let (eye, target) = self.eye_target();
        let forward = (target - eye).normalize_or(Vec3::Y);
        let right = forward.cross(Vec3::Z).normalize_or(Vec3::X);
        let up = right.cross(forward);
        let far = match &self.camera {
            SceneCamera::Map(c) => c.distance * 4.0 + 100.0,
            SceneCamera::Menu(_) => 512.0,
        };
        crate::render::SceneFrame {
            view_proj: self.view_proj(aspect),
            lighting: self.lighting,
            eye,
            far,
            right,
            forward,
            up,
            time,
        }
    }
}

/// `Math::Vector3::FromYawPitch` (degrees): the direction a camera with this yaw and pitch faces.
pub fn from_yaw_pitch(yaw: f32, pitch: f32) -> Vec3 {
    let (y, p) = (yaw.to_radians(), pitch.to_radians());
    Vec3::new(-p.cos() * y.sin(), p.cos() * y.cos(), p.sin())
}

/// The game's projection (`FUN_14009db30`): `vfov` degrees vertically and `aspect × vfov`
/// horizontally (each angle used directly, not through `tan`), up to 16:9.
pub fn projection(vfov: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    const WIDE: f32 = 16.0 / 9.0;
    let hfov = if aspect <= WIDE {
        aspect * vfov
    } else {
        // Beyond 16:9 the original widens through tan/atan; inferred from the call shape.
        2.0 * ((WIDE * vfov * 0.5).to_radians().tan() * aspect / WIDE)
            .atan()
            .to_degrees()
    };
    let sx = 1.0 / (hfov * 0.5).to_radians().tan();
    let sy = 1.0 / (vfov * 0.5).to_radians().tan();
    // Right-handed view space, depth 0..1 (wgpu).
    let a = far / (near - far);
    Mat4::from_cols(
        Vec4::new(sx, 0.0, 0.0, 0.0),
        Vec4::new(0.0, sy, 0.0, 0.0),
        Vec4::new(0.0, 0.0, a, -1.0),
        Vec4::new(0.0, 0.0, a * near, 0.0),
    )
}

/// The main menu's camera (`FUN_14009dcd0`, `FUN_14009d8c0`): orbits `target` at `distance`,
/// facing `FromYawPitch(yaw, pitch)`. The menu sways the yaw by `2·cos(t·11.25°)`
/// (`FUN_1400a0bb0`); drawn with a 40° field of view, near 2, far 512 (`FUN_1400a0910`).
#[derive(Clone, Copy)]
pub struct MenuCamera {
    pub target: Vec3,
    pub base_yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    /// Sway phase in degrees.
    pub angle: f32,
}

impl MenuCamera {
    pub fn yaw(&self) -> f32 {
        self.base_yaw + 2.0 * self.angle.to_radians().cos()
    }

    pub fn update(&mut self, dt: f32) {
        self.angle += dt * 11.25;
    }

    pub fn eye(&self) -> Vec3 {
        self.target - from_yaw_pitch(self.yaw(), self.pitch) * self.distance
    }

    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        use glam::camera::rh::view::look_at_mat4;
        projection(40.0, aspect, 2.0, 512.0) * look_at_mat4(self.eye(), self.target, Vec3::Z)
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
    pub fn overview(terrain: &Terrain) -> Camera {
        Camera {
            target: Vec2::ZERO,
            yaw: 0.0,
            pitch: 60f32.to_radians(),
            distance: terrain.size as f32 * 1.1,
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

    pub fn focus(&self, terrain: &Terrain) -> Vec3 {
        self.target
            .extend(terrain.height_at(self.target.x, self.target.y).max(0.0))
    }

    pub fn eye(&self, terrain: &Terrain) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        self.focus(terrain) + Vec3::new(-sy * cp, -cy * cp, sp) * self.distance
    }

    pub fn view_proj(&self, terrain: &Terrain, aspect: f32) -> Mat4 {
        use glam::camera::rh::{proj::directx::perspective, view::look_at_mat4};
        let proj = perspective(45f32.to_radians(), aspect, 0.5, self.distance * 4.0 + 100.0);
        proj * look_at_mat4(self.eye(terrain), self.focus(terrain), Vec3::Z)
    }

    pub fn pan(&mut self, forward: f32, right: f32, terrain: &Terrain) {
        let (sy, cy) = self.yaw.sin_cos();
        let step = self.distance * 0.02;
        self.target += Vec2::new(sy, cy) * forward * step + Vec2::new(cy, -sy) * right * step;
        let half = terrain.size as f32 / 2.0;
        self.target = self.target.clamp(Vec2::splat(-half), Vec2::splat(half));
    }

    /// Pan opposite a screen-space drag, keeping the target in the camera's ground plane.
    pub fn pan_drag(&mut self, dx: f32, dy: f32, terrain: &Terrain) {
        let (sy, cy) = self.yaw.sin_cos();
        let right = Vec2::new(cy, -sy);
        let forward = Vec2::new(sy, cy);
        let scale = self.distance * 0.0015;
        self.target += right * (-dx * scale) + forward * (dy * scale);
        let half = terrain.size as f32 / 2.0;
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
