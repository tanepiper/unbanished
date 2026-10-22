//! A pool of concurrent voices, after the original `Audio::Device` (Runtime DLL): 64 channels
//! (`0x28` bytes each, flag bit 0 = in use, bit 1 = ended). `Device::Play` takes the first free
//! channel and returns null when all are busy; `PlayPlatform` starts one XAudio2 source voice at
//! `offset % duration`, with the given volume and a frequency ratio of `1.05946314^pitch` (pitch in
//! semitones), looping when the sound buffer's loop flag is set. `Device::Update` frees ended
//! channels and calls their end callbacks; `Device::Stop` stops, frees and calls back.
//!
//! The port plays every channel at once through rodio's mixer natively and through one Web Audio
//! graph (a gain node per channel into a master gain) in the browser. The end callback becomes the
//! list of channels `update` returns.
//!
//! Original: Runtime `0x1800e0ad0`, Runtime `0x1800e0ff0`, Runtime `0x1800e0d70`, Runtime
//! `0x1800e0a50`.

use std::sync::Arc;
use std::time::Duration;

use crate::Pcm;

/// `Audio::Device` channel count (loops in `FindFreeChannel`, `Play`, `Update`).
pub const CHANNELS: usize = 64;

/// `DAT_180164810`: one semitone (the twelfth root of two).
const SEMITONE: f32 = 1.059_463_1;

/// The playback speed for a pitch in semitones: one semitone is the twelfth root of two, as
/// XAudio2's frequency ratio.
///
/// Original: `FUN_180129df0(1.05946314, pitch)`.
pub fn pitch_ratio(semitones: f32) -> f32 {
    SEMITONE.powf(semitones)
}

/// A decoded sound buffer (`Audio::SoundBuffer`) that any number of channels can play at once;
/// the samples are shared, not copied, per channel.
#[derive(Clone)]
pub struct Sound {
    pcm: Arc<Pcm>,
    // SoundBuffer +0x20
    /// Whether the sound loops.
    pub looping: bool,
    #[cfg(target_arch = "wasm32")]
    web: std::rc::Rc<std::cell::RefCell<Option<web_sys::AudioBuffer>>>,
}

impl Sound {
    /// A sound from decoded samples and its loop flag.
    pub fn new(pcm: Pcm, looping: bool) -> Sound {
        Sound {
            pcm: Arc::new(pcm),
            looping,
            #[cfg(target_arch = "wasm32")]
            web: Default::default(),
        }
    }

    /// Load and decode a sound by resource reference (see [`crate::load_sound`]).
    pub fn load(
        archive: &unbanished_formats::pkg::Archive,
        reference: &str,
    ) -> Result<Sound, String> {
        let (pcm, looping) = crate::load_sound(archive, reference)?;
        Ok(Sound::new(pcm, looping))
    }

    /// The decoded samples.
    pub fn pcm(&self) -> &Pcm {
        &self.pcm
    }

    /// The length in frames (samples per channel).
    pub fn frames(&self) -> usize {
        self.pcm.samples.len() / self.pcm.channels.max(1) as usize
    }

    /// The length.
    pub fn duration(&self) -> Duration {
        Duration::from_secs_f64(self.frames() as f64 / self.pcm.sample_rate.max(1) as f64)
    }

    /// `PlayPlatform`'s start point: the requested time wrapped into the sound.
    pub fn wrap(&self, offset: Duration) -> Duration {
        let d = self.duration().as_secs_f64();
        if d <= 0.0 {
            return Duration::ZERO;
        }
        Duration::from_secs_f64(offset.as_secs_f64() % d)
    }
}

/// A playing channel. Handles carry a generation, so one kept after its channel was freed and
/// reused refers to nothing (calls on it do nothing).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Channel {
    index: u8,
    generation: u32,
}

impl Channel {
    /// The channel's number, 0 to 63.
    pub fn index(self) -> usize {
        self.index as usize
    }

    #[cfg(test)]
    pub(crate) fn test(n: u32) -> Channel {
        Channel {
            index: (n % 64) as u8,
            generation: n,
        }
    }
}

/// Channel bookkeeping without any output: first-free allocation and generations.
#[derive(Debug)]
pub struct Slots<T> {
    slots: Vec<Option<(u32, T)>>,
    generation: u32,
}

impl<T> Default for Slots<T> {
    fn default() -> Self {
        Slots {
            slots: (0..CHANNELS).map(|_| None).collect(),
            generation: 0,
        }
    }
}

impl<T> Slots<T> {
    /// `FindFreeChannel`: the lowest free index, or `None` when all 64 are in use.
    pub fn insert(&mut self, value: T) -> Option<Channel> {
        let index = self.slots.iter().position(Option::is_none)?;
        self.generation = self.generation.wrapping_add(1);
        self.slots[index] = Some((self.generation, value));
        Some(Channel {
            index: index as u8,
            generation: self.generation,
        })
    }

    /// The value of a channel, or `None` when the handle is stale.
    pub fn get(&self, c: Channel) -> Option<&T> {
        match self.slots.get(c.index())? {
            Some((g, v)) if *g == c.generation => Some(v),
            _ => None,
        }
    }

    /// The value of a channel, to change it, or `None` when the handle is stale.
    pub fn get_mut(&mut self, c: Channel) -> Option<&mut T> {
        match self.slots.get_mut(c.index())? {
            Some((g, v)) if *g == c.generation => Some(v),
            _ => None,
        }
    }

    /// Free a channel and return its value.
    pub fn remove(&mut self, c: Channel) -> Option<T> {
        self.get(c)?;
        self.slots[c.index()].take().map(|(_, v)| v)
    }

    /// The channels in use, with their values.
    pub fn iter(&self) -> impl Iterator<Item = (Channel, &T)> {
        self.slots.iter().enumerate().filter_map(|(i, s)| {
            s.as_ref().map(|(g, v)| {
                (
                    Channel {
                        index: i as u8,
                        generation: *g,
                    },
                    v,
                )
            })
        })
    }

    /// The number of channels in use.
    pub fn len(&self) -> usize {
        self.slots.iter().filter(|s| s.is_some()).count()
    }

    /// Whether no channel is in use.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// How a channel starts (`Device::Play`'s time and floats).
#[derive(Debug, Clone, Copy)]
pub struct PlayParams {
    /// Start time, wrapped into the sound.
    pub offset: Duration,
    /// The volume, from 0 to 1.
    pub volume: f32,
    /// Semitones.
    pub pitch: f32,
    /// Defaults to the sound's own loop flag.
    pub looping: Option<bool>,
}

impl Default for PlayParams {
    fn default() -> Self {
        PlayParams {
            offset: Duration::ZERO,
            volume: 1.0,
            pitch: 0.0,
            looping: None,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::Device;
#[cfg(target_arch = "wasm32")]
pub use web::Device;

#[cfg(not(target_arch = "wasm32"))]
mod native;

#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(test)]
mod tests;
