//! Audio resources: `Audio::SoundEffect` (Runtime), and the game's `MusicList`,
//! `AnimAudioEvent`, `AmbientEmitterDescription` and `AmbientEnvironmentDescription`.
//! See docs/AUDIO.md.

use crate::crc::type_id;
use crate::crs::{self, Reader};
use crate::error::{format_err, Result};

/// One playable variant of a sound (0x18 bytes: buffer, pitch range, volume).
#[derive(Debug, Clone, PartialEq)]
pub struct SoundVariant {
    /// `Audio\…\X.rsc:resource` SoundBuffer.
    pub buffer: Option<String>,
    /// The lowest pitch, in semitones. `GetSoundBuffer` draws uniformly in
    /// `[pitch_min, pitch_max]`.
    pub pitch_min: f32,
    /// The highest pitch, in semitones.
    pub pitch_max: f32,
    /// The volume.
    pub volume: f32,
}

/// A sound of the effect's timeline: starts `start` seconds after the effect.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectSound {
    /// When it starts, in seconds after the effect.
    pub start: f32,
    /// The variants.
    pub variants: Vec<SoundVariant>,
}

/// A sound effect: a timeline of sounds, each with variants, a pitch range and a volume, and the
/// radius within which it can be heard.
///
/// Original: `Audio::SoundEffect::Serialize`, Runtime `0x1800e32f0`.
#[derive(Debug, Clone, PartialEq)]
pub struct SoundEffect {
    // +0x20
    /// Audible distance (3D effects).
    pub radius: f32,
    // +0x24
    /// Full-volume distance.
    pub inner_radius: f32,
    /// The timeline's sounds.
    pub sounds: Vec<EffectSound>,
    // +0x28
    /// The effect's length in microseconds; 0 when a sound loops (the effect never ends). The
    /// engine computes it when it compiles the effect.
    ///
    /// Original: `Audio::SoundEffect::Compile`, Runtime `0x1800e31f0`.
    pub max_length: i64,
}

/// Read a [`SoundEffect`] from the bytes of its resource.
pub fn sound_effect(data: &[u8]) -> Result<SoundEffect> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("SoundEffect") {
            return format_err(format!("expected SoundEffect, found {:#010x}", h.type_id));
        }
        r.block(|r, version| {
            let radius = r.f32()?;
            let inner_radius = r.f32()?;
            let sounds = (0..r.count()?)
                .map(|_| {
                    let start = r.f32()?;
                    let variants = (0..r.count()?)
                        .map(|_| {
                            Ok(SoundVariant {
                                buffer: r.external_ref()?,
                                pitch_min: r.f32()?,
                                pitch_max: r.f32()?,
                                volume: r.f32()?,
                            })
                        })
                        .collect::<Result<_>>()?;
                    Ok(EffectSound { start, variants })
                })
                .collect::<Result<_>>()?;
            let max_length = if version >= 1 { r.i64()? } else { 0 };
            Ok(SoundEffect {
                radius,
                inner_radius,
                sounds,
                max_length,
            })
        })
    })?;
    Ok(value)
}

/// One `MusicList` track (0x18 bytes): played when the month and the town's mood are in range.
///
/// Original: `FUN_1400a95b0`.
#[derive(Debug, Clone, PartialEq)]
pub struct MusicTrack {
    /// The music's sound effect.
    pub effect: Option<String>,
    // +0x8, +0xc
    /// Mood range (health and happiness fractions, averaged).
    pub mood: (f32, f32),
    // +0x10, +0x14
    /// Month range, inclusive.
    pub months: (i32, i32),
}

/// `MusicList` (`Game/MusicTracks.rsc`).
pub fn music_list(data: &[u8]) -> Result<Vec<MusicTrack>> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("MusicList") {
            return format_err(format!("expected MusicList, found {:#010x}", h.type_id));
        }
        r.block(|r, _| {
            (0..r.count()?)
                .map(|_| {
                    let mood = (r.f32()?, r.f32()?);
                    let months = (r.i32()?, r.i32()?);
                    Ok(MusicTrack {
                        effect: r.external_ref()?,
                        mood,
                        months,
                    })
                })
                .collect()
        })
    })?;
    Ok(value)
}

/// `AnimAudioEvent`: an animation event that plays a 3D effect at the entity (sound type 4) with
/// probability `probability`.
///
/// Original: `FUN_14008d310`.
#[derive(Debug, Clone, PartialEq)]
pub struct AnimAudioEvent {
    /// The effect.
    pub effect: Option<String>,
    /// The probability that it plays.
    pub probability: f32,
}

/// Read a [`AnimAudioEvent`] from the bytes of its resource.
pub fn anim_audio_event(data: &[u8]) -> Result<AnimAudioEvent> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("AnimAudioEvent") {
            return format_err(format!(
                "expected AnimAudioEvent, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            Ok(AnimAudioEvent {
                effect: r.external_ref()?,
                probability: r.f32()?,
            })
        })
    })?;
    Ok(value)
}

// +0x20
/// `AmbientEmitterDescription`: the effects an ambient emitter feeds, and whether activation starts
/// them.
///
/// Original: `FUN_140043b10`.
#[derive(Debug, Clone, PartialEq)]
pub struct AmbientEmitterDescription {
    /// The effects.
    pub effects: Vec<Option<String>>,
    /// Whether activation starts them.
    pub autostart: bool,
}

/// Read a [`AmbientEmitterDescription`] from the bytes of its resource.
pub fn ambient_emitter_description(data: &[u8]) -> Result<AmbientEmitterDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("AmbientEmitterDescription") {
            return format_err(format!(
                "expected AmbientEmitterDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            let effects = (0..r.count()?)
                .map(|_| r.external_ref())
                .collect::<Result<_>>()?;
            Ok(AmbientEmitterDescription {
                effects,
                autostart: r.bool()?,
            })
        })
    })?;
    Ok(value)
}

/// One terrain class of an `AmbientEnvironmentDescription` (0x28 bytes).
#[derive(Debug, Clone, PartialEq)]
pub struct AmbientTerrain {
    /// Cell types (bit = map cell type) this class counts.
    pub cell_mask: u32,
    /// When set, only cells whose entity uses this template count.
    pub entity: Option<String>,
    /// The warm effect, played at `t` of the temperature cross-fade.
    pub warm: Option<String>,
    /// The cold effect, played at `1 - t` of the temperature cross-fade.
    pub cold: Option<String>,
    /// The weight it adds for each cell counted.
    pub weight: i32,
}

/// `AmbientEnvironmentDescription`.
///
/// Original: `FUN_1400442b0`.
#[derive(Debug, Clone, PartialEq)]
pub struct AmbientEnvironmentDescription {
    /// The terrain classes.
    pub terrains: Vec<AmbientTerrain>,
    // +0x30, +0x34; +0x18
    /// Environment range of the cross-fade.
    pub fade: (f32, f32),
}

/// Read a [`AmbientEnvironmentDescription`] from the bytes of its resource.
pub fn ambient_environment_description(data: &[u8]) -> Result<AmbientEnvironmentDescription> {
    let (_, value) = crs::parse(data, |r: &mut Reader, h| {
        if h.type_id != type_id("AmbientEnvironmentDescription") {
            return format_err(format!(
                "expected AmbientEnvironmentDescription, found {:#010x}",
                h.type_id
            ));
        }
        r.block(|r, _| {
            let terrains = (0..r.count()?)
                .map(|_| {
                    Ok(AmbientTerrain {
                        cell_mask: r.u32()?,
                        entity: r.external_ref()?,
                        warm: r.external_ref()?,
                        cold: r.external_ref()?,
                        weight: r.i32()?,
                    })
                })
                .collect::<Result<_>>()?;
            Ok(AmbientEnvironmentDescription {
                terrains,
                fade: (r.f32()?, r.f32()?),
            })
        })
    })?;
    Ok(value)
}
