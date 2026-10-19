#![doc = include_str!("../README.md")]
#![doc(
    html_logo_url = "https://tanepiper.github.io/unbanished/assets/un-banished-icon.svg",
    html_favicon_url = "https://tanepiper.github.io/unbanished/favicon.svg"
)]

mod bits;
mod decoder;
mod mdct;
mod tables;
mod vlc;

pub use decoder::Decoder;

/// `WAVE_FORMAT_WMAUDIO2`.
pub const WMAV2: u16 = 0x161;

/// Encoder flags implied by xWMA, which carries no codec extradata: exponent VLCs, bit
/// reservoir and variable block lengths, with the widest block-size range.
pub const XWMA_FLAGS: u16 = 0x1f;

/// Stream parameters, mostly from the `WAVEFORMATEX` header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Params {
    /// Format tag; must be [`WMAV2`].
    pub tag: u16,
    /// The number of channels: 1 or 2.
    pub channels: u16,
    /// The sample rate, in samples per second.
    pub sample_rate: u32,
    /// The average data rate, in bytes per second.
    pub avg_bytes_per_sec: u32,
    /// Packet size in bytes.
    pub block_align: u16,
    /// The codec's `flags2` word (extradata bytes 4..6 in ASF); [`XWMA_FLAGS`] for xWMA.
    pub flags: u16,
}

impl Params {
    /// Parameters for an xWMA stream.
    pub fn xwma(channels: u16, sample_rate: u32, avg_bytes_per_sec: u32, block_align: u16) -> Self {
        Params {
            tag: WMAV2,
            channels,
            sample_rate,
            avg_bytes_per_sec,
            block_align,
            flags: XWMA_FLAGS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Why a stream could not be decoded.
pub enum Error {
    /// A valid stream using a feature this decoder does not implement.
    Unsupported(&'static str),
    /// Corrupt or inconsistent data.
    Invalid(&'static str),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Error::Unsupported(m) => write!(f, "unsupported WMA stream: {m}"),
            Error::Invalid(m) => write!(f, "invalid WMA data: {m}"),
        }
    }
}

impl std::error::Error for Error {}

/// A result with this crate's [`Error`].
pub type Result<T> = std::result::Result<T, Error>;

/// Decode a whole stream of `block_align`-sized packets to interleaved 16-bit PCM.
///
/// Like FFmpeg, packets that fail to decode are skipped, and the output ends with one extra
/// frame holding the final overlap tail.
pub fn decode(params: &Params, packets: &[u8]) -> Result<Vec<i16>> {
    let mut dec = Decoder::new(params)?;
    let mut pcm = Vec::new();
    for packet in packets.chunks(usize::from(params.block_align)) {
        let _ = dec.decode_packet(packet, &mut pcm);
    }
    dec.flush(&mut pcm);
    Ok(to_i16(&pcm))
}

/// Convert `[-1, 1)` float samples to 16-bit with rounding and clipping.
pub fn to_i16(samples: &[f32]) -> Vec<i16> {
    samples
        .iter()
        .map(|&s| (s * 32768.0).round_ties_even().clamp(-32768.0, 32767.0) as i16)
        .collect()
}
