//! The sound scene: the layer above the [`Device`](crate::Device) that plays sound effects.
//!
//! - **256 effects.** Each one walks its effect's timeline and starts every sound whose start time
//!   falls in the frame, with a random variant and a random pitch, at the volume
//!   `variant × effect × kind`. When all its sounds have ended, the effect frees itself (and the 3D
//!   effect that started it).
//! - **4,096 effects in the world.** Each has a position or a list of emitters. Every update takes
//!   the loudest emitter (the volume falls in a straight line from the inner radius to the radius,
//!   plus the listener's reach), starts the effect when it can be heard, stops it when it cannot,
//!   and passes the volume on.
//! - **Kinds of sound:** 1 interface, 2 music, 3 ambient, 4 effects (kind 0 never plays). Each
//!   kind has a volume (by default 100, 50, 100 and 100) and can be paused.
//!
//! Handles are encoded as in the original: an effect is `index·2 | counter<<9`, an effect in the
//! world `index·2 | 1 | counter<<13`. A stale handle does nothing. See
//! [Audio](https://tanepiper.github.io/unbanished/topics/audio).
//!
//! Original: `Audio::Scene` in the Runtime.
mod effects;
mod emitters;
pub use effects::{Effect, EffectCache, Prepared};

// The effects are at Scene +0x10 (0x70 bytes each), the 3D effects at +0x20 (0x40 bytes), the
// paused kinds at +0x30. See docs/AUDIO.md.

use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use glam::Vec3;
use unbanished_formats::audio::SoundEffect;

use crate::device::{Channel, PlayParams, Sound};

/// `0x100` and `0x1000` slots (`FindFreeEffect`, `FindFree3DEffect`).
const EFFECTS: usize = 0x100;
const EFFECTS_3D: usize = 0x1000;
/// Sounds per effect (`+0x20`, eight channel pointers; `+0x60`, eight flag bytes).
const SOUNDS: usize = 8;
/// `DAT_1801647cc`: 2^-32.
const UNIT: f32 = 2.328_306_4e-10;
/// `DAT_180164870`: a 3D effect is not restarted within `max_length / 20` of itself.
const RETRIGGER_DIVISOR: f32 = 20.0;

/// The kinds of sound. Each kind has its own volume in the options. Kind 0 never plays.
///
/// Original: `Audio::Scene::SoundType`.
pub mod sound_type {
    /// Interface sounds.
    pub const UI: u32 = 1;
    /// Music.
    pub const MUSIC: u32 = 2;
    /// Ambient sounds: weather, rivers, forests.
    pub const AMBIENT: u32 = 3;
    /// Sound effects in the world.
    pub const EFFECT: u32 = 4;
}

/// What the scene needs from the channel pool (implemented by [`crate::Device`]).
pub trait Mixer {
    /// Start a sound on a free channel, or return `None` when no channel is free.
    fn play(&mut self, sound: &Sound, params: PlayParams) -> Option<Channel>;
    /// Stop a channel.
    fn stop(&mut self, channel: Channel);
    /// Change a channel's volume.
    fn set_volume(&mut self, channel: Channel, volume: f32);
}

impl Mixer for crate::Device {
    fn play(&mut self, sound: &Sound, params: PlayParams) -> Option<Channel> {
        crate::Device::play(self, sound, params).ok().flatten()
    }
    fn stop(&mut self, channel: Channel) {
        crate::Device::stop(self, channel)
    }
    fn set_volume(&mut self, channel: Channel, volume: f32) {
        crate::Device::set_volume(self, channel, volume)
    }
}

/// The volume of each kind of sound, from 0 to 100, as in the Audio options.
///
/// Original: `Audio::Configuration`; its constructor `Configuration::Configuration`, Runtime
/// `0x1800e0700`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Volumes {
    /// Interface sounds.
    pub ui: i32,
    /// Music.
    pub music: i32,
    /// Ambient sounds.
    pub ambient: i32,
    /// Sound effects.
    pub effect: i32,
}

impl Default for Volumes {
    fn default() -> Self {
        Volumes {
            ui: 100,
            music: 50,
            ambient: 100,
            effect: 100,
        }
    }
}

impl Volumes {
    /// `Scene::Update`'s per-type table: type 0 is silent.
    fn table(&self) -> [f32; 5] {
        [
            0.0,
            self.ui as f32 / 100.0,
            self.music as f32 / 100.0,
            self.ambient as f32 / 100.0,
            self.effect as f32 / 100.0,
        ]
    }
}

/// One playing effect (`0x70` bytes).
struct Effect2D {
    effect: Rc<Effect>,
    kind: u32,
    /// `+0x10`: time into the effect (µs).
    elapsed: i64,
    /// `+0x18`: volume set by the 3D effect that drives it (1 otherwise).
    volume: f32,
    /// `+0x1c`: volume of the last variant started.
    variant_volume: f32,
    channels: [Option<Channel>; SOUNDS],
    /// Bit 0 started, bit 1 ended.
    flags: [u8; SOUNDS],
    handle: u32,
    /// `+0x6c`: cleared on start and unpause; the first update's window then begins at 0.
    started: bool,
}

/// One positional effect (`0x40` bytes).
struct Effect3D {
    position: Vec3,
    kind: u32,
    effect: Rc<Effect>,
    /// `+0x18`: µs.
    elapsed: i64,
    /// `+0x20`: the 2D effect playing it, or 0.
    playing: u32,
    handle: u32,
    /// `+0x28`.
    volume: f32,
    /// `+0x30`: positions and volumes; empty means `position` at volume 1.
    emitters: Vec<(Vec3, f32)>,
}

/// The sound scene: up to 256 playing effects and 4,096 effects in the world, their emitters,
/// and the volumes of the kinds of sound. [`Scene::update`] mixes them every frame.
///
/// Original: `Audio::Scene`.
pub struct Scene {
    /// `+0x00`: the frame's simulation time step (µs), the minimum retrigger window.
    step: i64,
    /// `+0x08`: handle counter, starting at 1.
    counter: u32,
    effects: Vec<Option<Effect2D>>,
    effects3d: Vec<Option<Effect3D>>,
    /// `+0x30`: paused sound types.
    paused: [bool; 5],
    /// The volume of each kind of sound.
    pub volumes: Volumes,
}

impl Default for Scene {
    fn default() -> Self {
        Scene {
            step: 0,
            counter: 1,
            effects: (0..EFFECTS).map(|_| None).collect(),
            effects3d: (0..EFFECTS_3D).map(|_| None).collect(),
            paused: [false; 5],
            volumes: Volumes::default(),
        }
    }
}

fn index2d(handle: u32) -> usize {
    (handle >> 1 & 0xff) as usize
}

fn index3d(handle: u32) -> usize {
    (handle >> 1 & 0xfff) as usize
}

/// `GetVolumeForPosition` (`0x1800e2720`).
fn volume_for_position(at: Vec3, radius: f32, inner: f32, listener: Vec3) -> f32 {
    let d = (at - listener).length();
    if radius < d {
        0.0
    } else if inner <= d {
        1.0 - (d - inner) / (radius - inner)
    } else {
        1.0
    }
}

fn micros(seconds: f32) -> i64 {
    (seconds as f64 * 1e6) as i64
}

impl Scene {
    fn effect_mut(&mut self, handle: u32) -> Option<&mut Effect2D> {
        self.effects
            .get_mut(index2d(handle))?
            .as_mut()
            .filter(|e| (e.handle ^ handle) & 0xffff_fe00 == 0)
    }

    /// `PlayEffect`: the first free slot, starting `start` µs into the effect. Returns 0 when all
    /// 256 are busy.
    ///
    /// Original: Runtime `0x1800e17b0`.
    pub fn play_effect(&mut self, effect: &Rc<Effect>, kind: u32, start: i64) -> u32 {
        let Some(i) = self.effects.iter().position(Option::is_none) else {
            return 0;
        };
        self.counter = self.counter.wrapping_add(1);
        let handle = ((i as u32) * 2) | (self.counter << 9);
        self.effects[i] = Some(Effect2D {
            effect: effect.clone(),
            kind,
            elapsed: start,
            volume: 1.0,
            variant_volume: 1.0,
            channels: [None; SOUNDS],
            flags: [0; SOUNDS],
            handle,
            started: false,
        });
        handle
    }

    /// `IsEffectPlaying`.
    pub fn is_effect_playing(&self, handle: u32) -> bool {
        self.effects
            .get(index2d(handle))
            .and_then(Option::as_ref)
            .is_some_and(|e| (e.handle ^ handle) & 0xffff_fe00 == 0)
    }

    /// `StopEffect`: stop its channels and free it.
    pub fn stop_effect(&mut self, mixer: &mut dyn Mixer, handle: u32) {
        if let Some(e) = self.effect_mut(handle) {
            for c in e.channels.iter().flatten() {
                mixer.stop(*c);
            }
            self.effects[index2d(handle)] = None;
        }
    }

    /// `StopAll`: every 2D effect (3D effects stay and restart when audible).
    pub fn stop_all(&mut self, mixer: &mut dyn Mixer) {
        for slot in &mut self.effects {
            if let Some(e) = slot.take() {
                for c in e.channels.iter().flatten() {
                    mixer.stop(*c);
                }
            }
        }
    }

    /// `PauseEffect`: stop the channels; started sounds are not restarted.
    fn pause_effect(e: &mut Effect2D, mixer: &mut dyn Mixer) {
        for (c, f) in e.channels.iter_mut().zip(&mut e.flags) {
            if let Some(ch) = c.take() {
                mixer.stop(ch);
                *f &= !2;
            }
        }
    }

    /// `Pause(type, paused)`.
    pub fn pause(&mut self, mixer: &mut dyn Mixer, kind: u32, paused: bool) {
        let k = kind as usize;
        if k >= self.paused.len() || self.paused[k] == paused {
            return;
        }
        for e in self.effects.iter_mut().flatten().filter(|e| e.kind == kind) {
            if paused {
                Self::pause_effect(e, mixer);
            } else {
                e.started = false;
            }
        }
        self.paused[k] = paused;
    }

    /// `IsPaused(type)`.
    pub fn is_paused(&self, kind: u32) -> bool {
        self.paused.get(kind as usize).copied().unwrap_or(false)
    }

    /// The device's end callback (`OnSoundEnded`): mark the sound ended.
    pub fn on_ended(&mut self, channel: Channel) {
        for e in self.effects.iter_mut().flatten() {
            for (c, f) in e.channels.iter_mut().zip(&mut e.flags) {
                if *c == Some(channel) {
                    *c = None;
                    *f |= 2;
                }
            }
        }
    }

    /// `Update(dt, step, random, listener, reach)`. `dt` advances the effects; `step` is the
    /// simulation step of the frame; `reach` extends every 3D effect's radius (the camera's
    /// distance from its pivot).
    ///
    /// Original: Runtime `0x1800e1f10`.
    pub fn update(
        &mut self,
        mixer: &mut dyn Mixer,
        dt: i64,
        step: i64,
        rand: &mut dyn FnMut() -> u32,
        listener: Vec3,
        reach: f32,
    ) {
        self.step = step;
        // 3D effects.
        for i in 0..EFFECTS_3D {
            let Some(e) = &self.effects3d[i] else {
                continue;
            };
            if self.paused[e.kind as usize % 5] {
                continue;
            }
            let volume = self.attenuated_volume(e, listener, reach);
            let (handle, kind, effect, playing, elapsed) =
                (e.handle, e.kind, e.effect.clone(), e.playing, e.elapsed);
            let mut playing = playing;
            if volume == 0.0 && playing != 0 {
                self.stop_effect(mixer, playing);
                playing = 0;
            } else if volume > 0.0 && playing == 0 {
                playing = self.play_effect(&effect, kind, elapsed);
            }
            if let Some(p) = self.effect_mut(playing) {
                p.volume = volume;
            }
            let e = self.effects3d[i].as_mut().unwrap();
            e.playing = playing;
            if playing == 0 && effect.max_length() != 0 && effect.max_length() <= e.elapsed {
                self.stop_effect_3d(mixer, handle);
            } else {
                e.elapsed += dt;
            }
        }
        // 2D effects.
        let volumes = self.volumes.table();
        for i in 0..EFFECTS {
            let Some(e) = self.effects[i].as_mut() else {
                continue;
            };
            if self.paused[e.kind as usize % 5] {
                continue;
            }
            let from = if e.started { e.elapsed } else { 0 };
            e.started = true;
            let to = e.elapsed + dt;
            let mut done = false;
            if e.kind != 0 {
                let type_volume = volumes[e.kind as usize % 5];
                let mut ended = 0;
                for (s, sound) in e.effect.desc.sounds.iter().enumerate().take(SOUNDS) {
                    let at = micros(sound.start);
                    if at < from || to <= at {
                        // Playing sounds follow the effect's volume.
                        if e.flags[s] & 1 != 0 && e.flags[s] & 2 == 0 {
                            if let Some(c) = e.channels[s] {
                                mixer.set_volume(c, e.variant_volume * e.volume * type_volume);
                            }
                        }
                    } else {
                        // In the window: start it (`GetSoundBuffer`: a random variant and a
                        // random pitch). The original does not check whether the sound already
                        // started; after an unpause the window reopens at 0, so paused sounds
                        // restart at the effect's elapsed time, which resumes them.
                        let n = sound.variants.len() as u32;
                        let v = if n == 0 { 0 } else { (rand() % n) as usize };
                        let r = rand();
                        let Some(variant) = sound.variants.get(v) else {
                            e.flags[s] |= 3;
                            ended += 1;
                            continue;
                        };
                        let pitch = (variant.pitch_max - variant.pitch_min) * r as f32 * UNIT
                            + variant.pitch_min;
                        e.variant_volume = variant.volume;
                        let channel = e.effect.buffers[s][v].as_ref().and_then(|buffer| {
                            mixer.play(
                                buffer,
                                PlayParams {
                                    offset: Duration::from_micros(e.elapsed.max(0) as u64),
                                    volume: variant.volume * e.volume * type_volume,
                                    pitch,
                                    looping: None,
                                },
                            )
                        });
                        e.channels[s] = channel;
                        e.flags[s] |= 1;
                        if channel.is_none() {
                            // No buffer or no free channel: the sound is over at once.
                            e.flags[s] |= 2;
                        }
                    }
                    if e.flags[s] & 2 != 0 {
                        ended += 1;
                    }
                }
                done = ended == e.effect.desc.sounds.len().min(SOUNDS);
            }
            e.elapsed = to;
            if done {
                let handle = e.handle;
                self.effects[i] = None;
                for slot in &mut self.effects3d {
                    if slot.as_ref().is_some_and(|f| f.playing == handle) {
                        *slot = None;
                    }
                }
            }
        }
    }

    /// `GetAttenuatedVolume` (`0x1800e2630`).
    fn attenuated_volume(&self, e: &Effect3D, listener: Vec3, reach: f32) -> f32 {
        let radius = reach + e.effect.desc.radius;
        let inner = e.effect.desc.inner_radius;
        let v = if e.emitters.is_empty() {
            volume_for_position(e.position, radius, inner, listener)
        } else {
            e.emitters.iter().fold(0.0f32, |best, (p, w)| {
                best.max(volume_for_position(*p, radius, inner, listener) * w)
            })
        };
        v * e.volume
    }

    /// Effects currently held (tests, debugging).
    pub fn active(&self) -> (usize, usize) {
        (
            self.effects.iter().flatten().count(),
            self.effects3d.iter().flatten().count(),
        )
    }
}

#[cfg(test)]
mod tests;
