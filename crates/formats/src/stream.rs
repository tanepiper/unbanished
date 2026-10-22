//! Block streams (`IO::CompressedStream`): a table of up-to-32 KiB blocks, each LZF or stored.
//!
//! ```text
//! u32 n
//! n x { u32 offset, u16 compSize (0 = stored), u16 rawSize }
//! blocks...
//! ```
//! `IO::CompressedStream::Open` requires `4 + 8n + sum(compSize or rawSize) == len`.

use crate::error::{format_err, Result};
use crate::lzf;

/// The largest expanded block, 32 KiB.
pub const BLOCK_SIZE: usize = 0x8000;

/// Expand a block stream.
pub fn decode(buf: &[u8]) -> Result<Vec<u8>> {
    let rd = |at: usize, n: usize| -> Result<&[u8]> {
        buf.get(at..at + n)
            .ok_or(crate::Error::Truncated { at, wanted: n })
    };
    let count = u32::from_le_bytes(rd(0, 4)?.try_into().unwrap()) as usize;
    let table_end = 4usize.saturating_add(
        count
            .checked_mul(8)
            .ok_or(crate::Error::Format("block count overflow".into()))?,
    );
    if table_end > buf.len() {
        return format_err("block table exceeds stream");
    }
    let mut blocks = Vec::with_capacity(count);
    let mut total = table_end;
    for k in 0..count {
        let e = rd(4 + 8 * k, 8)?;
        let offset = u32::from_le_bytes(e[0..4].try_into().unwrap()) as usize;
        let comp = u16::from_le_bytes(e[4..6].try_into().unwrap()) as usize;
        let raw = u16::from_le_bytes(e[6..8].try_into().unwrap()) as usize;
        total += if comp != 0 { comp } else { raw };
        blocks.push((offset, comp, raw));
    }
    if total != buf.len() {
        return format_err(format!(
            "stream size mismatch: table says {total}, have {}",
            buf.len()
        ));
    }
    let mut out = Vec::with_capacity(blocks.iter().map(|b| b.2).sum());
    for (offset, comp, raw) in blocks {
        if comp != 0 {
            lzf::decompress_into(rd(offset, comp)?, &mut out, BLOCK_SIZE)?;
        } else {
            out.extend_from_slice(rd(offset, raw)?);
        }
    }
    Ok(out)
}

/// The writing side of [`decode`]: `raw` in 32 KiB blocks, each LZF unless storing it is no
/// larger. Offsets count from the start of the stream.
pub fn encode(raw: &[u8]) -> Vec<u8> {
    let blocks: Vec<(Vec<u8>, usize)> = raw
        .chunks(BLOCK_SIZE)
        .map(|b| {
            let packed = lzf::compress(b);
            if packed.len() < b.len() {
                (packed, b.len())
            } else {
                (Vec::new(), b.len())
            }
        })
        .collect();
    let mut out = Vec::new();
    out.extend_from_slice(&(blocks.len() as u32).to_le_bytes());
    let mut offset = 4 + 8 * blocks.len();
    for (packed, len) in &blocks {
        out.extend_from_slice(&(offset as u32).to_le_bytes());
        out.extend_from_slice(&(packed.len() as u16).to_le_bytes());
        out.extend_from_slice(&(*len as u16).to_le_bytes());
        offset += if packed.is_empty() {
            *len
        } else {
            packed.len()
        };
    }
    for (chunk, (packed, _)) in raw.chunks(BLOCK_SIZE).zip(&blocks) {
        out.extend_from_slice(if packed.is_empty() { chunk } else { packed });
    }
    out
}
