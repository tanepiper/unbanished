//! Packed per-cell surface data, uploaded as the terrain's `size × size` image and kept at
//! MapComponent.
//!
//! The image is BGRA8 (format 5), so in memory each texel is the bytes `[dirt, rock, light, snow]`
//! and the terrain pixel shader (`TerrainMaterial_ps`, sampler `transitionMap`) reads them as `.z`,
//! `.y`, `.x`, `.w`:
//! - Dirt (`.z`, lerps grass → dirt): 255 below height -0.5, otherwise squared Perlin noise;
//!   raised towards 255 above height 8.
//! - Rock (`.y`, lerps → triplanar rock): from the cell normal's up component (0 when nearly flat,
//!   255 when steep); raised towards 255 above height 7.
//! - Light (`.x`, multiplies the ambient term): one minus the mean facing of a radius-10
//!   neighbourhood, scaled by the smoothed normal's up component.
//! - Snow (`.w`): left 0 here.
//!
//! Float operations follow the disassembly order so results are bit-exact.
//!
//! Original: `FUN_1400d4910`, `FUN_1400b1c50`.
// +0x140

use super::Generator;
use banished_core::noise::PerlinNoise;

const NOISE_SCALE: f32 = 0.25;
const NOISE_OCTAVES: f32 = 4.0;
const LOW: f32 = -0.5;
const FLAT: f32 = 0.999;
const STEEP: f32 = 0.85;
/// `DAT_1401787f0`, the f32 difference `0.999 - 0.85`.
const SLOPE_RANGE: f32 = FLAT - STEEP;
const R_HEIGHT: f32 = 8.0;
const G_HEIGHT: f32 = 7.0;
const HEIGHT_RAMP: f32 = 3.0;
const NORMAL_Z: f32 = 2.0;

type Vec3 = [f32; 3];

/// `Math::Vector3::Normalize`: multiply by `1 / length`; a zero vector is returned unchanged.
fn normalize(v: Vec3) -> Vec3 {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len == 0.0 {
        return v;
    }
    let inv = 1.0 / len;
    [v[0] * inv, v[1] * inv, v[2] * inv]
}

fn dot(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// `cvttss2si` to 64 bits, keeping the low byte.
fn byte(v: f32) -> u32 {
    (v as i64 as u64 & 0xff) as u32
}

/// `clamp((h - from) / 3, 0, 1) * 255` as a byte.
fn height_ramp(h: f32, from: f32) -> u32 {
    // comiss order in the original keeps NaN, as `clamp` does.
    let t = ((h - from) / HEIGHT_RAMP).clamp(0.0, 1.0);
    byte(t * 255.0)
}

struct Cell {
    pos: Vec3,
    normal: Vec3,
}

impl Generator {
    fn sample(&self, x: i32, y: i32) -> f32 {
        let hi = self.size;
        let x = x.clamp(0, hi);
        let y = y.clamp(0, hi);
        self.grid[(y * (hi + 1) + x) as usize]
    }

    /// `FUN_1400d4620`: the normal from central differences, with 2 as the up component.
    fn cell_normal(&self, x: i32, y: i32) -> Vec3 {
        let dx = self.sample(x + 1, y) - self.sample(x - 1, y);
        let dy = self.sample(x, y + 1) - self.sample(x, y - 1);
        normalize([-dx, -dy, NORMAL_Z])
    }

    /// Build the packed surface image (`dirt | rock << 8 | light << 16`). Consumes two random
    /// values, so it must run after `run` exactly as the game does.
    ///
    /// Original: `FUN_1400d4910`.
    pub fn surface_map(&mut self) -> Vec<u32> {
        let seed_y = self.rng.rand_tol(0xffffff);
        let seed_x = self.rng.rand_tol(0xffffff);
        let noise = PerlinNoise::new(NOISE_SCALE, NOISE_OCTAVES, seed_x, seed_y);
        let n = self.size;
        let mut cells = Vec::with_capacity((n * n) as usize);
        for y in 0..n {
            for x in 0..n {
                cells.push(Cell {
                    pos: [x as f32, y as f32, self.sample(x, y)],
                    normal: self.cell_normal(x, y),
                });
            }
        }

        // Each texel depends only on the finished cells (no Random), so the rows are computed
        // on several threads (native); every row is written by one thread, in the same order of
        // operations as one pass.
        let mut out = vec![0u32; (n * n) as usize];
        let mut rows: Vec<(i32, &mut [u32])> = out
            .chunks_mut(n as usize)
            .enumerate()
            .map(|(y, row)| (y as i32, row))
            .collect();
        banished_formats::par::map_mut(&mut rows, |(y, row)| {
            for x in 0..n {
                row[x as usize] = surface_texel(&cells, &noise, n, x, *y);
            }
        });
        out
    }
}

/// One texel of `surface_map`: dirt from the noise below `LOW`, rock from the slope, both
/// raised by height, then the light (occlusion over a radius of 10 cells times the smoothed
/// normal's z).
fn surface_texel(cells: &[Cell], noise: &PerlinNoise, n: i32, x: i32, y: i32) -> u32 {
    let c = &cells[(y * n + x) as usize];
    let h = c.pos[2];
    let up = c.normal[2];
    let mut r = if LOW <= h {
        let v = noise.generate(x as f32 * NOISE_SCALE, y as f32 * NOISE_SCALE);
        let v = v * 0.5 + 0.5;
        byte(v * v * 255.0)
    } else {
        0xff
    };
    let mut g = if FLAT > up && up > STEEP {
        byte((1.0 - (up - STEEP) / SLOPE_RANGE) * 255.0)
    } else if STEEP >= up {
        0xff
    } else {
        0
    };
    if h > R_HEIGHT {
        r = r.max(height_ramp(h, R_HEIGHT));
    }
    if h > G_HEIGHT {
        g = g.max(height_ramp(h, G_HEIGHT));
    }
    let mut occlusion = 0.0f32;
    let mut count = 0.0f32;
    let mut smooth = [0.0f32; 3];
    // The original visits rows -10..=9 but columns -10..=10.
    for dy in -10..10 {
        let ny = y + dy;
        if ny < 0 || ny >= n {
            continue;
        }
        for dx in -10..=10 {
            let nx = x + dx;
            if nx < 0 || nx >= n || dx * dx + dy * dy > 100 {
                continue;
            }
            let o = &cells[(ny * n + nx) as usize];
            let dir = normalize([
                o.pos[0] - c.pos[0],
                o.pos[1] - c.pos[1],
                o.pos[2] - c.pos[2],
            ]);
            let d = dot(c.normal, dir);
            occlusion += if d > 0.0 { d } else { 0.0 };
            count += 1.0;
            if dx.abs() <= 1 && dy.abs() <= 1 {
                for (s, n) in smooth.iter_mut().zip(o.normal) {
                    *s += n;
                }
            }
        }
    }
    let smooth = normalize(smooth);
    let b = byte((1.0 - occlusion / count) * smooth[2] * 255.0);
    ((g & 0xff) << 8 | (r & 0xff)) | b << 16
}
