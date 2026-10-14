//! The five generation stages. Each function names the original it ports.

use super::Generator;
use crate::noise::PerlinNoise;

fn sqrtf(x: f32) -> f32 {
    x.sqrt()
}

/// CRT `powf` (computed in double precision, rounded to float).
fn powf(base: f32, exp: f32) -> f32 {
    (base as f64).powf(exp as f64) as f32
}

impl Generator {
    /// Stage 1, `FUN_1400d3aa0`: two noise fields blended by a logistic falloff from the centre.
    pub fn stage1_noise(&mut self) {
        let w = self.width();
        self.grid = vec![0.0; w * w];
        let a = self.rng.rand_tol(0xffffff);
        let b = self.rng.rand_tol(0xffffff);
        let near = PerlinNoise::new(0.5, 5.0, b, a);
        let a = self.rng.rand_tol(0xffffff);
        let b = self.rng.rand_tol(0xffffff);
        let far = PerlinNoise::new(0.25, 5.0, b, a);
        let half = self.size >> 1;
        let radius = self.size as f32 * 0.25 + 16.0;
        for y in 0..w as i32 {
            let dy = (y - half) * (y - half);
            let (yf, ynear, yfar) = (y as f32, y as f32 * 0.25, y as f32 * 0.1);
            let _ = yf;
            for x in 0..w as i32 {
                let dist = sqrtf(((x - half) * (x - half) + dy) as f32);
                let p = powf(1.1, -(dist - radius));
                let weight = 1.0 / (p + 1.0);
                let g1 = near.generate(x as f32 * 0.25, ynear);
                let low = (g1 + 1.0) * 8.0;
                let g2 = far.generate(x as f32 * 0.1, yfar);
                let mut high = (g2 + 0.25) * 48.0 + g1;
                if high < 2.0 {
                    high = 2.0;
                }
                self.grid[y as usize * w + x as usize] = (1.0 - weight) * low + high * weight;
            }
        }
    }

    pub fn stage2_records(&mut self) {
        todo!("FUN_1400d2560 / FUN_1400d2620 / FUN_1400d2430")
    }

    pub fn stage3_shape(&mut self) {
        todo!("FUN_1400d2930 / FUN_1400d3de0")
    }

    pub fn stage4_downhill(&mut self) {
        todo!("FUN_1400d1550")
    }

    pub fn stage5_smooth(&mut self) {
        todo!("FUN_1400d4120 / FUN_1400d1820 x2")
    }
}
