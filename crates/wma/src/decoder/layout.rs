//! The block layout of a stream: the block sizes and the coefficient band edges for each.

use super::*;

pub(super) fn layout(sample_rate: u32, frame_len_bits: u32, k: u32, high_freq: f32) -> Layout {
    let frame_len = 1usize << frame_len_bits;
    let block_len = frame_len >> k;

    let arm = match sample_rate {
        44_100.. => Some(0),
        32_000.. => Some(1),
        22_050.. => Some(2),
        _ => None,
    };
    let hard = (frame_len_bits - BLOCK_MIN_BITS)
        .checked_sub(k)
        .filter(|&a| a < 3)
        .zip(arm)
        .and_then(|(a, arm)| tables::EXPONENT_BAND_EDGES[arm][a as usize]);
    let exponent_bands: Vec<usize> = match hard {
        Some(edges) => edges.windows(2).map(|w| usize::from(w[1] - w[0])).collect(),
        None => {
            // Critical-band edges rounded to multiples of four coefficients.
            let mut bands = Vec::new();
            let mut last = 0;
            for &f in &tables::CRITICAL_FREQS {
                let b = u64::from(sample_rate);
                let pos = (((block_len as u64 * 2 * u64::from(f)) + (b << 1)) / (4 * b)) << 2;
                let pos = (pos as usize).min(block_len);
                if pos > last {
                    bands.push(pos - last);
                }
                if pos >= block_len {
                    break;
                }
                last = pos;
            }
            bands
        }
    };

    let coefs_end = (frame_len - frame_len * 9 / 100) >> k;
    let high_band_start =
        ((block_len as f32 * 2.0 * high_freq) / sample_rate as f32 + 0.5) as usize;
    let mut high_bands = Vec::new();
    let mut pos = 0;
    for &w in &exponent_bands {
        let start = pos.max(high_band_start);
        pos += w;
        let end = pos.min(coefs_end);
        if end > start {
            high_bands.push(end - start);
        }
    }
    Layout {
        exponent_bands,
        coefs_end,
        high_band_start,
        high_bands,
    }
}
