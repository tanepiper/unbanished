#![doc = include_str!("../README.md")]
#![doc(
    html_logo_url = "https://tanepiper.github.io/unbanished/assets/un-banished-icon.svg",
    html_favicon_url = "https://tanepiper.github.io/unbanished/favicon.svg"
)]

mod device;
pub mod scene;

pub use device::{pitch_ratio, Channel, Device, PlayParams, Slots, Sound, CHANNELS};
pub use scene::{sound_type, Effect, EffectCache, Mixer, Prepared, Scene, Volumes};

use unbanished_formats::pkg::Archive;
use unbanished_formats::types::{self, SoundBuffer};

/// Decoded samples.
#[derive(Clone)]
pub struct Pcm {
    /// The number of channels.
    pub channels: u16,
    /// The sample rate, in samples per second.
    pub sample_rate: u32,
    /// The samples, interleaved, from -1 to 1.
    pub samples: Vec<f32>,
}

/// Decode a `SoundBuffer`; `stream` supplies the packets of streamed music (its `.str` entry).
pub fn decode(sound: &SoundBuffer, stream: Option<&[u8]>) -> Result<Pcm, String> {
    let f = &sound.format;
    let params = unbanished_wma::Params::xwma(
        f.channels,
        f.sample_rate,
        f.avg_bytes_per_sec,
        f.block_align,
    );
    let packets = stream.unwrap_or(&sound.data);
    let pcm = unbanished_wma::decode(&params, packets).map_err(|e| format!("decode: {e:?}"))?;
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
