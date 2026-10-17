//! `Utility::PerlinNoise` (Runtime-steam-x64.dll 0x18010ad80..0x18010b520): hashed value noise with
//! 3x3 smoothing and cosine interpolation, summed over octaves.
//!
//! Float operations follow the disassembly order exactly (single precision throughout); `cosf` is
//! the Runtime's C runtime one ([`crate::crt::cosf`]).

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PerlinNoise {
    /// Amplitude multiplier per octave (`powf(persistence, octave)`).
    pub persistence: f32,
    /// Octave count; the loop runs while `(float)i < octaves`.
    pub octaves: f32,
    pub seed_x: i32,
    pub seed_y: i32,
}

const PI: f32 = std::f32::consts::PI;

fn cosf(x: f32) -> f32 {
    crate::crt::cosf(x)
}

/// `powf` for the exponents the generator uses (small non-negative integers).
fn powf(base: f32, exp: f32) -> f32 {
    (base as f64).powf(exp as f64) as f32
}

impl PerlinNoise {
    /// `PerlinNoise(float, float, int, int)` (0x18010b500).
    pub fn new(persistence: f32, octaves: f32, seed_x: i32, seed_y: i32) -> Self {
        PerlinNoise {
            persistence,
            octaves,
            seed_x,
            seed_y,
        }
    }

    #[inline]
    fn hash(&self, x: i32, y: i32) -> u32 {
        (self.seed_y.wrapping_add(y) as u32)
            .wrapping_mul(0x39)
            .wrapping_add(self.seed_x as u32)
            .wrapping_add(x as u32)
    }

    #[inline]
    fn value(n: u32) -> f32 {
        let n = n ^ n.wrapping_mul(0x2000);
        let m = n
            .wrapping_mul(n)
            .wrapping_mul(0x3d73)
            .wrapping_add(0xc0ae5)
            .wrapping_mul(n)
            .wrapping_add(0xd208dd0d)
            & 0x7fff_ffff;
        1.0 - m as f32 * 9.313_226e-10
    }

    /// `Random(int, int)` (0x18010ad80).
    pub fn random(&self, x: i32, y: i32) -> f32 {
        Self::value(self.hash(x, y))
    }

    /// `SmoothNoise(int, int)` (0x18010ae50): corners/16 + edges/8 + centre/4, in the original's
    /// summation order.
    pub fn smooth_noise(&self, x: i32, y: i32) -> f32 {
        let c = self.hash(x, y);
        let v = |d: i32| Self::value(c.wrapping_add(d as u32));
        let corners = v(-0x38) + v(-0x3a) + v(0x38) + v(0x3a);
        let sides = v(1) + v(-1) + v(-0x39) + v(0x39);
        corners * 0.0625 + sides * 0.125 + v(0) * 0.25
    }

    /// `Generate(float, float)` (0x18010b240).
    pub fn generate(&self, x: f32, y: f32) -> f32 {
        let mut total = 0.0f32;
        let mut i = 0i32;
        while (i as f32) < self.octaves {
            let freq = powf(2.0, i as f32);
            let (fx, fy) = (freq * x, freq * y);
            let (ix, iy) = (fx as i32, fy as i32);
            let a = (1.0 - cosf((fx - ix as f32) * PI)) * 0.5;
            let b = (1.0 - cosf((fy - iy as f32) * PI)) * 0.5;
            let top = self.smooth_noise(ix, iy) * (1.0 - a) + self.smooth_noise(ix + 1, iy) * a;
            let bottom =
                self.smooth_noise(ix, iy + 1) * (1.0 - a) + self.smooth_noise(ix + 1, iy + 1) * a;
            let v = top * (1.0 - b) + bottom * b;
            total += v * powf(self.persistence, i as f32);
            i += 1;
        }
        total
    }
}

#[cfg(test)]
mod golden {
    //! Values produced by the original Runtime DLL under emulation (tools/emu/check_random.py).
    use super::*;

    #[test]
    fn matches_original_engine() {
        let n = PerlinNoise::new(0.5, 5.0, 12345, -6789);
        for (x, y, expected) in [
            (0.0f32, 0.0f32, 0.6994447112083435f32),
            (1.25, 7.5, -0.048602547496557236),
            (100.75, 33.1, 0.27891477942466736),
            (511.5, 255.25, 0.1838667392730713),
        ] {
            assert_eq!(n.generate(x, y).to_bits(), expected.to_bits(), "({x}, {y})");
        }
    }
}
