//! LZF decompression (`System::Compression::Decompress`).
//!
//! Original: Runtime `0x1800c6ed0`.

use crate::error::{format_err, Result};

/// Decompress one LZF block, appending to `out`. Back-references may only reach
/// bytes produced by this call, matching the engine's per-block windows.
pub fn decompress_into(src: &[u8], out: &mut Vec<u8>, max_len: usize) -> Result<()> {
    let base = out.len();
    let mut i = 0;
    while i < src.len() {
        let ctrl = src[i] as usize;
        i += 1;
        if ctrl < 0x20 {
            let n = ctrl + 1;
            let Some(lit) = src.get(i..i + n) else {
                return format_err("LZF literal run past end of input");
            };
            out.extend_from_slice(lit);
            i += n;
        } else {
            let mut n = ctrl >> 5;
            if n == 7 {
                let Some(&extra) = src.get(i) else {
                    return format_err("LZF truncated length");
                };
                n += extra as usize;
                i += 1;
            }
            let Some(&lo) = src.get(i) else {
                return format_err("LZF truncated offset");
            };
            i += 1;
            let back = ((ctrl & 0x1F) << 8) + lo as usize + 1;
            if back > out.len() - base {
                return format_err("LZF back-reference before block start");
            }
            let from = out.len() - back;
            for k in 0..n + 2 {
                let b = out[from + k];
                out.push(b);
            }
        }
        if out.len() - base > max_len {
            return format_err("LZF block exceeds its window");
        }
    }
    Ok(())
}

/// Compress one block in the LZF format [`decompress_into`] reads: literal runs of up to 32
/// bytes, and back-references of 3 to 264 bytes up to 8 KiB back, found by a 3-byte hash.
/// Any valid encoding loads; this need not match the engine's choice of matches.
pub fn compress(src: &[u8]) -> Vec<u8> {
    const MAX_BACK: usize = 1 << 13;
    const MAX_LEN: usize = 7 + 255 + 2;
    let mut out = Vec::with_capacity(src.len() + src.len() / 32 + 1);
    let mut table = vec![usize::MAX; 1 << 14];
    let hash = |i: usize| {
        let v = (src[i] as u32) << 16 | (src[i + 1] as u32) << 8 | src[i + 2] as u32;
        (v.wrapping_mul(2654435761) >> 18) as usize & ((1 << 14) - 1)
    };
    // Where the open literal run's control byte is.
    let mut run: Option<usize> = None;
    let mut i = 0;
    while i < src.len() {
        let mut matched = 0;
        let mut back = 0;
        if i + 2 < src.len() {
            let h = hash(i);
            let cand = table[h];
            table[h] = i;
            if cand != usize::MAX && i - cand <= MAX_BACK && src[cand..cand + 3] == src[i..i + 3] {
                let limit = (src.len() - i).min(MAX_LEN);
                matched = 3;
                while matched < limit && src[cand + matched] == src[i + matched] {
                    matched += 1;
                }
                back = i - cand - 1;
            }
        }
        if matched >= 3 {
            run = None;
            let n = matched - 2;
            if n < 7 {
                out.push((n << 5 | back >> 8) as u8);
            } else {
                out.push((7 << 5 | back >> 8) as u8);
                out.push((n - 7) as u8);
            }
            out.push(back as u8);
            // Index the positions the match covers so later matches can find them.
            for k in i + 1..(i + matched).min(src.len().saturating_sub(2)) {
                table[hash(k)] = k;
            }
            i += matched;
        } else {
            match run {
                Some(at) if out[at] < 0x1f => out[at] += 1,
                _ => {
                    run = Some(out.len());
                    out.push(0);
                }
            }
            out.push(src[i]);
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compress_round_trips() {
        let mut data = Vec::new();
        for k in 0..20_000u32 {
            data.extend_from_slice(&(k % 300).to_le_bytes());
            data.push((k * 7 % 13) as u8);
        }
        data.extend(std::iter::repeat_n(9u8, 1000));
        for block in data.chunks(0x8000) {
            let packed = compress(block);
            assert!(packed.len() < block.len());
            let mut out = Vec::new();
            decompress_into(&packed, &mut out, 0x8000).unwrap();
            assert_eq!(out, block);
        }
        let mut out = Vec::new();
        decompress_into(&compress(b"ab"), &mut out, 64).unwrap();
        assert_eq!(out, b"ab");
    }

    #[test]
    fn literal_and_backref() {
        // "abc" literal, then copy 4 bytes from 3 back -> "abcabca"
        let src = [0x02, b'a', b'b', b'c', 0x40, 0x02];
        let mut out = Vec::new();
        decompress_into(&src, &mut out, 64).unwrap();
        assert_eq!(out, b"abcabca");
    }

    #[test]
    fn rejects_bad_backref() {
        let mut out = Vec::new();
        assert!(decompress_into(&[0x20, 0x05], &mut out, 64).is_err());
    }
}
