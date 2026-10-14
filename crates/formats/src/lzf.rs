//! LZF decompression (`System::Compression::Decompress`, Runtime 0x1800c6ed0).

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

#[cfg(test)]
mod tests {
    use super::*;

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
