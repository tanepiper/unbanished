//! The game's audio: the 64-channel `Device`, the `Audio::Scene` above it, loaded
//! SoundEffects, and the UI sound slots (`UserInterface::SetSound`, set from the global sounds
//! of `FUN_14009f280` by `FUN_1400778f0`). See docs/AUDIO.md.
//!
//! Deviation: the original passes the game's Random (`Game +0xd8`) to `Scene::Update` and the
//! music player, at moments that depend on the frame rate and on when sounds end. The port
//! gives audio its own Random so that the simulation stays reproducible.

use std::rc::Rc;

use banished_audio::{Device, Effect, EffectCache, Scene};
use banished_formats::pkg::Archive;
use banished_game::random::Random;
use glam::Vec3;

pub use banished_audio::sound_type;

/// UI sound slots (`UserInterface +0x98 + slot·8`): 2 begin drag, 3 end drag, 4 button down,
/// 5 button up, 6 alert. Slots 2, 3 and 6 are set but never played in 1.0.7 (no caller of
/// `BeginDragDrop`, no `Play(6)`); see docs/AUDIO.md.
const UI_SOUNDS: [(usize, &str); 5] = [
    (2, "Audio/UI/BeginDragEffect.rsc:resource"),
    (3, "Audio/UI/EndDragEffect.rsc:resource"),
    (4, "Audio/UI/ButtonDownEffect.rsc:resource"),
    (5, "Audio/UI/ButtonUpEffect.rsc:resource"),
    (6, "Audio/UI/AlertEffect.rsc:resource"),
];

/// Global sounds the game plays directly (`DAT_1401cbde8 +0x80..+0x98`).
pub mod global {
    pub const PLACEMENT_MOVE: &str = "Audio/UI/PlacementMoveEffect.rsc:resource";
    pub const PLACEMENT_THUNK: &str = "Audio/UI/PlacementThunkEffect.rsc:resource";
    pub const PLACEMENT_ERROR: &str = "Audio/UI/PlacementErrorEffect.rsc:resource";
    pub const WARNING: &str = "Audio/UI/WarningEffect.rsc:resource";
}

pub struct Audio {
    pub device: Device,
    pub scene: Scene,
    cache: EffectCache,
    rng: Random,
}

impl Default for Audio {
    fn default() -> Self {
        Audio {
            device: Device::default(),
            scene: Scene::default(),
            cache: EffectCache::default(),
            rng: Random::new(0x5eed),
        }
    }
}

impl Audio {
    pub fn unlock(&mut self) {
        self.device.unlock();
    }

    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub fn set_master_volume(&mut self, volume: f32) {
        self.device.set_master_volume(volume);
    }

    pub fn effect(&mut self, archive: &Archive, reference: &str) -> Option<Rc<Effect>> {
        self.cache.get(archive, reference)
    }

    /// `Scene::PlayEffect`.
    pub fn play(&mut self, archive: &Archive, reference: &str, kind: u32) -> u32 {
        match self.effect(archive, reference) {
            Some(e) => self.scene.play_effect(&e, kind, 0),
            None => 0,
        }
    }

    /// `Scene::PlayEffect3D`.
    pub fn play_3d(&mut self, archive: &Archive, reference: &str, kind: u32, at: Vec3) -> u32 {
        match self.effect(archive, reference) {
            Some(e) => self.scene.play_effect_3d(&e, kind, 0, at),
            None => 0,
        }
    }

    /// `UserInterface::Play(slot)`: the slot's effect as a UI sound.
    pub fn play_ui(&mut self, archive: &Archive, slot: usize) {
        if let Some((_, r)) = UI_SOUNDS.iter().find(|(s, _)| *s == slot) {
            self.play(archive, r, sound_type::UI);
        }
    }

    /// `Scene::Pause(type, paused)`.
    pub fn pause(&mut self, kind: u32, paused: bool) {
        self.scene.pause(&mut self.device, kind, paused);
    }

    pub fn stop_all(&mut self) {
        self.scene.stop_all(&mut self.device);
    }

    /// `Device::Update` with the end callbacks into the scene.
    pub fn frame(&mut self) {
        for c in self.device.update() {
            self.scene.on_ended(c);
        }
    }

    /// `Scene::Update(dt, step, random, listener, reach)`.
    pub fn update(&mut self, dt: f32, step: f32, listener: Vec3, reach: f32) {
        let rng = &mut self.rng;
        self.scene.update(
            &mut self.device,
            (dt as f64 * 1e6) as i64,
            (step as f64 * 1e6) as i64,
            &mut || rng.rand(),
            listener,
            reach,
        );
    }

    /// A random number from the audio stream (music choice).
    pub fn rand_below(&mut self, n: u32) -> u32 {
        self.rng.rand_below(n)
    }
}
