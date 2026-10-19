//! Sound playback shared by the game and the viewer. Game audio is xWMA (WMA v2), decoded
//! in-process by `banished-wma`, then mixed on a 64-channel [`Device`] (the original
//! `Audio::Device`) with rodio natively or Web Audio in the browser. No exported files are needed.

mod device;
pub mod scene;

pub use device::{pitch_ratio, Channel, Device, PlayParams, Slots, Sound, CHANNELS};
pub use scene::{sound_type, Effect, EffectCache, Mixer, Prepared, Scene, Volumes};

use banished_formats::pkg::Archive;
use banished_formats::types::{self, SoundBuffer};

#[derive(Clone)]
pub struct Pcm {
    pub channels: u16,
    pub sample_rate: u32,
    /// Interleaved samples in [-1, 1).
    pub samples: Vec<f32>,
}

/// Decode a `SoundBuffer`; `stream` supplies the packets of streamed music (its `.str` entry).
pub fn decode(sound: &SoundBuffer, stream: Option<&[u8]>) -> Result<Pcm, String> {
    let f = &sound.format;
    let params = banished_wma::Params::xwma(
        f.channels,
        f.sample_rate,
        f.avg_bytes_per_sec,
        f.block_align,
    );
    let packets = stream.unwrap_or(&sound.data);
    let pcm = banished_wma::decode(&params, packets).map_err(|e| format!("decode: {e:?}"))?;
    Ok(Pcm {
        channels: f.channels,
        sample_rate: f.sample_rate,
        samples: pcm.into_iter().map(|s| s as f32 / 32768.0).collect(),
    })
}

/// Load and decode a sound by resource reference (e.g. `Audio\\Music\\Menu.rsc:music`), reading
/// the `.str` stream of streamed music. Returns the samples and the resource's loop flag.
pub fn load_sound(archive: &Archive, reference: &str) -> Result<(Pcm, bool), String> {
    let sound = types::sound_buffer(&archive.read_ref(reference).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let stream = if sound.streamed {
        Some(
            archive
                .read(&sound.stream_entry)
                .map_err(|e| e.to_string())?,
        )
    } else {
        None
    };
    Ok((decode(&sound, stream.as_deref())?, sound.looping))
}
