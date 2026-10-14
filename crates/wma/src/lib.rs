//! Pure-Rust decoder for Windows Media Audio v2 (`WAVE_FORMAT_WMAUDIO2`, tag 0x161), the codec
//! inside xWMA files. No dependencies and no `unsafe`, so it builds for `wasm32-unknown-unknown`.
//!
//! ```no_run
//! # let packets: &[u8] = &[];
//! let params = banished_wma::Params::xwma(2, 44_100, 6_000, 2_230);
//! let pcm: Vec<i16> = banished_wma::decode(&params, packets).unwrap(); // interleaved
//! ```
//!
//! For streaming, drive a [`Decoder`] one packet at a time and call [`Decoder::flush`] at the end.

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
    pub channels: u16,
    pub sample_rate: u32,
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
