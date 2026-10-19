//! 3D content drawn behind a loop's UI.

use glam::{Mat4, Vec2, Vec3, Vec4};

use crate::gfx::render::Lighting;
use crate::view::world::Terrain;

/// A terrain with its camera and lighting. `id` changes whenever `terrain` does, so the renderer
/// knows when to upload it again.
pub struct SceneView {
    pub id: u64,
    pub terrain: Terrain,
    pub models: Option<crate::gfx::models::SceneModels>,
    pub extras: Option<crate::view::scene_assets::SceneExtras>,
    /// Bumped when `extras`' decals change after the upload (the game's roads).
    pub decals_revision: u64,
    pub camera: SceneCamera,
    pub lighting: Lighting,
    pub highlight: crate::gfx::highlight::Geometry,
    /// StatusIconComponent billboards and the icon sheets they use (slice E).
    pub status_icons: Vec<crate::gfx::status_icons::StatusIcon>,
    pub icon_sheets: std::collections::BTreeMap<String, crate::gfx::status_icons::IconSheet>,
    /// Minimap overlay (`Game +0x148 → +0x28`): revision and RGBA rows, top row north.
    pub minimap_overlay: Option<(u64, u32, Vec<u8>)>,
    /// The tools' cell overlay (`FUN_140105d90`) and its texture.
    pub cell_overlay: Vec<crate::gfx::selection_overlay::OverlayVertex>,
    pub overlay_texture: Option<crate::gfx::selection_overlay::OverlayTexture>,
    /// This frame's rain and snow, and their materials' textures.
    pub weather: Vec<crate::view::weather::WeatherBatch>,
    pub weather_textures:
        std::collections::HashMap<String, Option<banished_formats::types::ImageBuffer>>,
    /// This frame's particles and the textures (diffuse, normal map) of their materials.
    pub particles: Vec<crate::view::particles::ParticleBatch>,
    pub particle_textures: std::collections::HashMap<
        String,
        (
            Option<banished_formats::types::ImageBuffer>,
            Option<banished_formats::types::ImageBuffer>,
        ),
    >,
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
            decals_revision: 0,
            camera,
            lighting,
            highlight: Default::default(),
            status_icons: Vec::new(),
            minimap_overlay: None,
            icon_sheets: Default::default(),
            cell_overlay: Vec::new(),
            overlay_texture: None,
            particles: Vec::new(),
            particle_textures: Default::default(),
            weather: Vec::new(),
            weather_textures: Default::default(),
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
    pub fn frame(&self, aspect: f32, time: f32) -> crate::gfx::render::SceneFrame {
        let (eye, target) = self.eye_target();
        let forward = (target - eye).normalize_or(Vec3::Y);
        let right = forward.cross(Vec3::Z).normalize_or(Vec3::X);
        let up = right.cross(forward);
        let far = match &self.camera {
            SceneCamera::Map(c) => c.distance * 4.0 + 100.0,
            SceneCamera::Menu(_) => 512.0,
        };
        crate::gfx::render::SceneFrame {
            view_proj: self.view_proj(aspect),
            lighting: self.lighting,
            eye,
            far,
            right,
            forward,
            up,
            time,
            hfov: horizontal_fov(40.0, aspect),
            vfov: 40.0,
            near: 2.0,
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
    let hfov = horizontal_fov(vfov, aspect);
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

/// `FUN_14009db30`'s horizontal field of view in degrees.
pub fn horizontal_fov(vfov: f32, aspect: f32) -> f32 {
    const WIDE: f32 = 16.0 / 9.0;
    if aspect <= WIDE {
        aspect * vfov
    } else {
        // Beyond 16:9 the original widens through tan/atan; inferred from the call shape.
        2.0 * ((WIDE * vfov * 0.5).to_radians().tan() * aspect / WIDE)
            .atan()
            .to_degrees()
    }
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

/// The game camera (`Game +0xe0`, built by `FUN_14009da80(frustum, yaw 0, pitch -40,
/// distance 25)` in `FUN_14009bfd0`, projected by `FUN_14009db30(40, 2, 512)`) and the part of
/// the gameplay controller (`Game +0x120`) that drives it each frame (`FUN_140104390`).
/// Angles are degrees in the original's convention: the camera faces
/// `from_yaw_pitch(yaw, pitch)` (negative pitch looks down) from `target − facing × distance`.
/// See docs/REFERENCE-OBSERVATIONS.md (start camera).
#[derive(Clone, Copy)]
pub struct Camera {
    /// `+0x1c`: pivot on the ground plane (the original keeps its z at 0).
    pub target: Vec2,
    /// `+0x28`, `+0x2c`, `+0x30`.
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    /// Controller `+0x3c`: zoom target (0 = take the camera's distance on the next update).
    pub distance_target: f32,
    /// Controller `+0x44`: pitch slider in 0..1 (PitchUp/PitchDown ±0.125; constructor
    /// `FUN_140107a90` sets 0.75).
    pub pitch_slider: f32,
    /// Fixed views (map captures) skip the controller.
    pub fixed: bool,
}

/// `DAT_1401788c4`, `DAT_140178918`: zoom range; `DAT_140178898`: distance per wheel step.
const ZOOM_MIN: f32 = 20.0;
const ZOOM_MAX: f32 = 130.0;
const ZOOM_STEP: f32 = 5.0;

impl Camera {
    /// A whole-map view for captures (not an original camera).
    pub fn overview(terrain: &Terrain) -> Camera {
        Camera {
            target: Vec2::ZERO,
            yaw: 0.0,
            pitch: -60.0,
            distance: terrain.size as f32 * 1.1,
            distance_target: terrain.size as f32 * 1.1,
            pitch_slider: 0.75,
            fixed: true,
        }
    }

    /// The camera a new game starts with.
    pub fn ground() -> Camera {
        Camera {
            target: Vec2::ZERO,
            yaw: 0.0,
            pitch: -40.0,
            distance: 25.0,
            distance_target: 0.0,
            pitch_slider: 0.75,
            fixed: false,
        }
    }

    pub fn focus(&self, _terrain: &Terrain) -> Vec3 {
        self.target.extend(0.0)
    }

    pub fn eye(&self, terrain: &Terrain) -> Vec3 {
        self.focus(terrain) - from_yaw_pitch(self.yaw, self.pitch) * self.distance
    }

    pub fn view_proj(&self, terrain: &Terrain, aspect: f32) -> Mat4 {
        use glam::camera::rh::view::look_at_mat4;
        projection(40.0, aspect, 2.0, 512.0)
            * look_at_mat4(self.eye(terrain), self.focus(terrain), Vec3::Z)
    }

    /// `FUN_140104390`'s zoom/pitch part, once per frame: the pitch target follows the zoom
    /// (`f = (d − 20) / 110`, `pitch = (−65p − 85(1−p))·f + (−25p − 60(1−p))·(1−f)`), and the
    /// camera eases 10% of the way to both. Then the pivot is clamped to
    /// `± n/2 · clamp(1 − (distance − 100) / 150)`.
    pub fn update(&mut self, terrain: &Terrain) {
        if self.fixed {
            return;
        }
        if self.distance_target == 0.0 {
            self.distance_target = self.distance;
        }
        let pitch_target = self.pitch_target();
        self.pitch = pitch_target * 0.1 + self.pitch * 0.9;
        self.distance = self.distance * 0.9 + self.distance_target * 0.1;
        self.clamp(terrain);
    }

    /// The pitch the controller eases towards (`+0x40`) for its zoom target and slider.
    pub fn pitch_target(&self) -> f32 {
        let p = self.pitch_slider;
        let f = (self.distance_target - ZOOM_MIN) / 110.0;
        (p * -65.0 - (1.0 - p) * 85.0) * f + (p * -25.0 - (1.0 - p) * 60.0) * (1.0 - f)
    }

    fn clamp(&mut self, terrain: &Terrain) {
        let factor = (1.0 - (self.distance - 100.0) / 150.0).clamp(0.0, 1.0);
        // Map cells `n` are half the terrain's cells.
        let bound = (terrain.size / 2) as f32 * 0.5 * factor;
        self.target = self.target.clamp(Vec2::splat(-bound), Vec2::splat(bound));
    }

    /// Ground-plane forward and right for the current yaw.
    fn axes(&self) -> (Vec2, Vec2) {
        let (s, c) = self.yaw.to_radians().sin_cos();
        (Vec2::new(-s, c), Vec2::new(c, s))
    }

    /// `FUN_140104390`'s movement: a direction from the held movement keys and the screen
    /// edges (x −1 left / +1 right, y −1 forward / +1 back), normalized, moves the pivot
    /// `f·70 + (1−f)·12` units per second with `f = (zoom target − 20) / 110`.
    pub fn travel(&mut self, direction: Vec2, dt: f32, terrain: &Terrain) {
        if direction == Vec2::ZERO {
            return;
        }
        if self.distance_target == 0.0 {
            self.distance_target = self.distance;
        }
        let f = (self.distance_target - ZOOM_MIN) / 110.0;
        let speed = f * 70.0 + (1.0 - f) * 12.0;
        let d = direction.normalize() * speed * dt;
        let (fw, rt) = self.axes();
        self.target += rt * d.x - fw * d.y;
        self.clamp(terrain);
    }

    /// RotateLeft / RotateRight held: 90° per second.
    pub fn turn(&mut self, direction: f32, dt: f32) {
        self.yaw += direction * 90.0 * dt;
    }

    /// Rotate-button drag (`KeyMap` value 7 held): `yaw −= mouse Δx · 0.2`. The original has no
    /// drag pitch; pitch follows the zoom and the PitchUp/PitchDown slider.
    pub fn rotate(&mut self, dx: f32, _dy: f32) {
        self.yaw -= dx * 0.2;
    }

    /// PitchUp / PitchDown pressed: the pitch slider that `update` blends the pitch target with
    /// moves 0.125, within 0..1.
    pub fn pitch(&mut self, steps: f32) {
        self.pitch_slider = (self.pitch_slider + steps * 0.125).clamp(0.0, 1.0);
    }

    /// Mouse wheel / ZoomIn / ZoomOut: the zoom target moves 5 (towards the ground for a positive
    /// wheel) within 20..130.
    pub fn zoom(&mut self, steps: f32) {
        if self.distance_target == 0.0 {
            self.distance_target = self.distance;
        }
        // The original only looks at the wheel's sign: one step per update.
        if steps != 0.0 {
            self.distance_target =
                (self.distance_target - steps.signum() * ZOOM_STEP).clamp(ZOOM_MIN, ZOOM_MAX);
        }
    }
}
