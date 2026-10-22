//! Sound buffers: the wave format and the xWMA data of a sound.

use super::*;

/// `WAVEFORMATEX` header carried by every sound. All shipped audio is xWMA (`tag == 0x161`).
#[derive(Debug, Clone, Copy)]
pub struct WaveFormat {
    /// The format tag: `0x161`, WMA v2, for every sound of the game.
    pub tag: u16,
    /// The number of channels.
    pub channels: u16,
    /// The sample rate.
    pub sample_rate: u32,
    /// The average data rate, in bytes per second.
    pub avg_bytes_per_sec: u32,
    /// The packet size in bytes.
    pub block_align: u16,
    /// The bits per sample.
    pub bits_per_sample: u16,
}

/// A sound: its format, its loop flag, its length, and its data in xWMA (or the entry of its
/// stream).
#[derive(Debug, Clone)]
pub struct SoundBuffer {
    /// Whether the sound loops.
    pub looping: bool,
    /// Streamed sounds keep their packets in a raw `.str` package entry named by `stream_entry`.
    pub streamed: bool,
    /// Original source file, e.g. `Audio\Animal\Chicken0.wav`.
    pub source: String,
    /// The package entry of a streamed sound's packets.
    pub stream_entry: String,
    /// The length in seconds.
    pub length_seconds: f32,
    /// The format header.
    pub format: WaveFormat,
    /// Raw `WAVEFORMATEX(TENSIBLE)` bytes (40).
    pub format_bytes: Vec<u8>,
    /// xWMA `dpds` table: cumulative decoded bytes per packet, u32 LE.
    pub packet_table: Vec<u8>,
    /// xWMA packets (empty when streamed).
    pub data: Vec<u8>,
}

impl SoundBuffer {
    /// The resource class name.
    pub const CLASS: &'static str = "SoundBuffer";

    /// `Audio::SoundBuffer::Serialize` + `PlatformSerialize`.
    ///
    /// Original: Runtime `0x1800e2cd0`, Runtime `0x1800e2e10`.
    pub fn read(r: &mut Reader) -> Result<Self> {
        r.block(|r, _| {
            let looping = r.bool()?;
            let streamed = r.bool()?;
            let source = r.string()?;
            let stream_entry = r.string()?;
            let length_seconds = r.f32()?;
            let data_len = r.count()?;
            let table_len = r.count()?;
            let packet_table = r.bytes(table_len)?.to_vec();
            let format_bytes = r.bytes(0x28)?.to_vec();
            let data = r.bytes(data_len)?.to_vec();
            let u16_at = |o: usize| u16::from_le_bytes([format_bytes[o], format_bytes[o + 1]]);
            let u32_at = |o: usize| u32::from_le_bytes(format_bytes[o..o + 4].try_into().unwrap());
            let format = WaveFormat {
                tag: u16_at(0),
                channels: u16_at(2),
                sample_rate: u32_at(4),
                avg_bytes_per_sec: u32_at(8),
                block_align: u16_at(12),
                bits_per_sample: u16_at(14),
            };
            Ok(SoundBuffer {
                looping,
                streamed,
                source,
                stream_entry,
                length_seconds,
                format,
                format_bytes,
                packet_table,
                data,
            })
        })
    }

    /// Wrap as a standard xWMA RIFF file (`RIFF/XWMA` with `fmt `, `dpds`, `data`), which FFmpeg
    /// and XAudio2 tooling read. `stream` supplies the packets for streamed sounds.
    pub fn to_xwma(&self, stream: Option<&[u8]>) -> Vec<u8> {
        fn chunk(out: &mut Vec<u8>, tag: &[u8; 4], body: &[u8]) {
            out.extend_from_slice(tag);
            out.extend_from_slice(&(body.len() as u32).to_le_bytes());
            out.extend_from_slice(body);
            if body.len() % 2 == 1 {
                out.push(0);
            }
        }
        let data = stream.unwrap_or(&self.data);
        let mut body = b"XWMA".to_vec();
        chunk(&mut body, b"fmt ", &self.format_bytes[..18]);
        chunk(&mut body, b"dpds", &self.packet_table);
        chunk(&mut body, b"data", data);
        let mut out = b"RIFF".to_vec();
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&body);
        out
    }
}
