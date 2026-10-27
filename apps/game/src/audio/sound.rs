//! The game's audio: the 64-channel `Device`, the `Audio::Scene` above it, loaded
//! SoundEffects, and the UI sound slots (`UserInterface::SetSound`, set from the global sounds
//! of `FUN_14009f280` by `FUN_1400778f0`). See docs/AUDIO.md.
//!
//! In a game, `Scene::Update` and the music player draw from the game's Random (`Game +0xd8`):
//! two draws for each sound that starts, a UI click's included. The caller passes that Random;
//! without it (the main menu, whose loop passes its own Random at `+0x168`) they draw from
//! the audio's own.

use std::rc::Rc;

use glam::Vec3;
use unbanished_audio::{Device, Effect, EffectCache, Scene};
use unbanished_formats::pkg::Archive;
use unbanished_game::random::Random;

pub use unbanished_audio::sound_type;

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
        #[allow(unused_mut)]
        let mut device = Device::default();
        // The game's unit tests are never heard, however they are started.
        #[cfg(all(test, not(target_arch = "wasm32")))]
        device.silence();
        Audio {
            device,
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

    /// The effects of `references` that are not in the cache yet.
    pub fn missing_effects<'a>(&self, references: &[&'a str]) -> Vec<&'a str> {
        self.cache.missing(references)
    }

    /// Put effects decoded elsewhere ([`unbanished_audio::EffectCache::prepare`]) in the cache.
    pub fn add_prepared(&mut self, prepared: unbanished_audio::Prepared) {
        self.cache.add(prepared);
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

    /// `Scene::Update(dt, step, random, listener, reach)`. Each sound that starts draws from
    /// `random`, the game's Random in a game, or else from the audio's own.
    pub fn update(
        &mut self,
        dt: f32,
        step: f32,
        listener: Vec3,
        reach: f32,
        random: Option<&mut Random>,
    ) {
        let rng = random.unwrap_or(&mut self.rng);
        self.scene.update(
            &mut self.device,
            (dt as f64 * 1e6) as i64,
            (step as f64 * 1e6) as i64,
            &mut || rng.rand(),
            listener,
            reach,
        );
    }

    /// A random number for the music choice: from `random`, the game's Random in a game, or
    /// else from the audio's own.
    pub fn rand_below(&mut self, n: u32, random: Option<&mut Random>) -> u32 {
        random.unwrap_or(&mut self.rng).rand_below(n)
    }

    /// The number [`Audio::rand_below`] would give next, without drawing it.
    pub fn peek_rand_below(&self, n: u32, random: Option<&Random>) -> u32 {
        random.unwrap_or(&self.rng).clone().rand_below(n)
    }
}
