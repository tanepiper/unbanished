//! CPU decoding of `ImageBuffer` levels to RGBA8, for targets without BC texture support.

use crate::types::ImageFormat;

fn rgb565(c: u16) -> [u8; 3] {
    let (r, g, b) = ((c >> 11) & 31, (c >> 5) & 63, c & 31);
    [
        (r << 3 | r >> 2) as u8,
        (g << 2 | g >> 4) as u8,
        (b << 3 | b >> 2) as u8,
    ]
}

fn bc1_palette(c0: u16, c1: u16, four_colour: bool) -> [[u8; 4]; 4] {
    let (p0, p1) = (rgb565(c0), rgb565(c1));
    let mix = |a: u16, b: u16, d: u16| -> [u8; 4] {
        let f = |i: usize| ((a * p0[i] as u16 + b * p1[i] as u16) / d) as u8;
        [f(0), f(1), f(2), 255]
    };
    let solid = |p: [u8; 3]| [p[0], p[1], p[2], 255];
    if c0 > c1 || four_colour {
        [solid(p0), solid(p1), mix(2, 1, 3), mix(1, 2, 3)]
    } else {
        [solid(p0), solid(p1), mix(1, 1, 2), [0, 0, 0, 0]]
    }
}

fn bc3_alpha(a0: u8, a1: u8) -> [u8; 8] {
    let (a0w, a1w) = (a0 as u16, a1 as u16);
    let mut t = [a0, a1, 0, 0, 0, 0, 0, 0];
    if a0 > a1 {
        for i in 1..7u16 {
            t[i as usize + 1] = (((7 - i) * a0w + i * a1w) / 7) as u8;
        }
    } else {
        for i in 1..5u16 {
            t[i as usize + 1] = (((5 - i) * a0w + i * a1w) / 5) as u8;
        }
        t[6] = 0;
        t[7] = 255;
    }
    t
}

/// Decode one level to tightly packed RGBA8 (`width * height * 4` bytes).
/// Returns `None` for formats that are not colour images (R32F, depth).
pub fn decode_rgba(format: ImageFormat, width: u32, height: u32, data: &[u8]) -> Option<Vec<u8>> {
    let (w, h) = (width.max(1) as usize, height.max(1) as usize);
    let mut out = vec![0u8; w * h * 4];
    match format {
        ImageFormat::Bgra8 => {
            for (o, i) in out.chunks_exact_mut(4).zip(data.chunks_exact(4)) {
                o.copy_from_slice(&[i[2], i[1], i[0], i[3]]);
            }
        }
        ImageFormat::R8 => {
            for (o, &v) in out.chunks_exact_mut(4).zip(data) {
                o.copy_from_slice(&[v, v, v, 255]);
            }
        }
        ImageFormat::A8 => {
            for (o, &v) in out.chunks_exact_mut(4).zip(data) {
                o.copy_from_slice(&[255, 255, 255, v]);
            }
        }
        ImageFormat::Bc1 | ImageFormat::Bc3 => {
            let bc3 = format == ImageFormat::Bc3;
            let size = if bc3 { 16 } else { 8 };
            let (bw, bh) = (w.div_ceil(4), h.div_ceil(4));
            for by in 0..bh {
                for bx in 0..bw {
                    let mut o = (by * bw + bx) * size;
                    let blk = data.get(o..o + size)?;
                    let alpha = bc3.then(|| {
                        let bits = u64::from_le_bytes([
                            blk[2], blk[3], blk[4], blk[5], blk[6], blk[7], 0, 0,
                        ]);
                        (bc3_alpha(blk[0], blk[1]), bits)
                    });
                    if bc3 {
                        o += 8;
                    }
                    let c0 = u16::from_le_bytes([data[o], data[o + 1]]);
                    let c1 = u16::from_le_bytes([data[o + 2], data[o + 3]]);
                    let bits = u32::from_le_bytes(data[o + 4..o + 8].try_into().unwrap());
                    let pal = bc1_palette(c0, c1, bc3);
                    for py in 0..4 {
                        for px in 0..4 {
                            let (x, y) = (bx * 4 + px, by * 4 + py);
                            if x >= w || y >= h {
                                continue;
                            }
                            let k = py * 4 + px;
                            let mut c = pal[((bits >> (2 * k)) & 3) as usize];
                            if let Some((table, abits)) = alpha {
                                c[3] = table[((abits >> (3 * k)) & 7) as usize];
                            }
                            let p = (y * w + x) * 4;
                            out[p..p + 4].copy_from_slice(&c);
                        }
                    }
                }
            }
        }
        ImageFormat::R32F | ImageFormat::R24G8 => return None,
    }
    Some(out)
}
