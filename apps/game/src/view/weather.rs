//! Rain and snow (`WeatherGeometry`, built by `FUN_1400d9450`, moved by `FUN_1400d8330`,
//! drawn by `FUN_1400d85f0`). See docs/ENVIRONMENT.md.
//!
//! A `Weather` description's particles fill a cube of `+0x48` units around the origin, each
//! with a velocity along the normalized direction (`+0x2c`) at `RandFloatTol × +0x3c + +0x38`;
//! every update moves them and wraps them back into the cube. The cube is drawn as instances
//! tiled around the camera (five a side, those in the view), each particle a quad: a camera-
//! facing square of `+0x44` for snow, a streak from its last position to this one for rain
//! (`+0x40`). The alpha is the effect's intensity, faded with the depth between `+0x58`..`+0x54`
//! and `+0x50`..`+0x4c`. Presentation only: the particles have their own Random.

use banished_formats::weather::Weather;
use banished_game::random::Random;
use glam::{Mat4, Vec3};

#[derive(Clone, Copy, Debug)]
struct Drop {
    pos: Vec3,
    prev: Vec3,
    velocity: Vec3,
}

/// One weather slot's particles.
pub struct Precipitation {
    pub desc: Weather,
    drops: Vec<Drop>,
}

/// What the renderer needs for one slot this frame.
#[derive(Clone, Debug, Default)]
pub struct WeatherBatch {
    pub material: String,
    /// Per vertex: position, the previous position, uv, 8 floats, 4 a particle (indices
    /// `0, 1, 2, 0, 2, 3`).
    pub vertices: Vec<f32>,
    /// The cube's size (`+0x48`); the renderer tiles it around the camera ([`tiles`]).
    pub size: f32,
    /// Quad half size, streaks (rain), the depth fade (`near0, near1, far0, far1`) and alpha.
    pub half: f32,
    pub streaks: bool,
    pub fade: [f32; 4],
    pub alpha: f32,
}

impl Precipitation {
    /// `FUN_1400d9450`.
    pub fn new(desc: Weather, seed: u32) -> Precipitation {
        let mut rng = Random::new(seed);
        let size = desc.floats_44[1];
        let d = Vec3::from(desc.direction).normalize_or_zero();
        let drops = (0..desc.count.max(0))
            .map(|_| {
                let z = rng.rand_float_tol();
                let y = rng.rand_float_tol();
                let x = rng.rand_float_tol();
                let pos = Vec3::new(x * size, size * y, size * z) * 0.5;
                let speed = rng.rand_float_tol() * desc.speed[1] + desc.speed[0];
                let velocity = d * speed;
                Drop {
                    pos,
                    prev: pos - velocity * 0.016,
                    velocity,
                }
            })
            .collect();
        Precipitation { desc, drops }
    }

    /// `FUN_1400d8330`: move, then wrap into the cube (the last position follows).
    pub fn update(&mut self, dt: f32) {
        let size = self.desc.floats_44[1];
        let half = size * 0.5;
        for d in &mut self.drops {
            d.prev = d.pos;
            d.pos += d.velocity * dt;
            for k in 0..3 {
                while half < d.pos[k] {
                    d.pos[k] -= size;
                    d.prev[k] -= size;
                }
                while d.pos[k] < -half {
                    d.pos[k] += size;
                    d.prev[k] += size;
                }
            }
        }
    }

    /// `FUN_1400d85f0`'s geometry for `intensity`: the particles' quads.
    pub fn batch(&self, intensity: f32) -> Option<WeatherBatch> {
        let material = self.desc.material.clone()?;
        if intensity <= 0.0 || self.drops.is_empty() {
            return None;
        }
        let mut vertices = Vec::with_capacity(self.drops.len() * 32);
        let streaks = self.desc.flag;
        for d in &self.drops {
            let prev = if streaks { d.prev } else { d.pos };
            // The original's corners: `p + R − U`, `p + R + U`, `q − R + U`, `q − R − U`, uv
            // `(0, 0)`, `(1, 0)`, `(1, 1)`, `(0, 1)`; the shader reads the end and side from
            // the uv.
            for (u, v) in [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)] {
                vertices
                    .extend_from_slice(&[d.pos.x, d.pos.y, d.pos.z, prev.x, prev.y, prev.z, u, v]);
            }
        }
        let f = &self.desc.floats_44;
        Some(WeatherBatch {
            material,
            vertices,
            size: f[1],
            half: f[0] * 0.5,
            streaks,
            fade: [f[5], f[4], f[3], f[2]],
            alpha: intensity,
        })
    }
}

/// `FUN_1400d85f0`: the centres of the cubes of the 5 × 5 × 5 around the camera at `eye`
/// that are in the view.
pub fn tiles(size: f32, eye: Vec3, view_proj: &Mat4) -> Vec<[f32; 3]> {
    let base = (eye / size).floor() * size;
    let mut tiles = Vec::new();
    for k in -2..=2 {
        for j in -2..=2 {
            for i in -2..=2 {
                let corner = base + Vec3::new(i as f32, j as f32, k as f32) * size;
                if box_visible(view_proj, corner, corner + Vec3::splat(size)) {
                    tiles.push((corner + Vec3::splat(size * 0.5)).to_array());
                }
            }
        }
    }
    tiles
}

/// A box is drawn unless all its corners are outside one clip plane.
fn box_visible(view_proj: &Mat4, lo: Vec3, hi: Vec3) -> bool {
    let corners: Vec<glam::Vec4> = (0..8)
        .map(|i| {
            let p = Vec3::new(
                if i & 1 == 0 { lo.x } else { hi.x },
                if i & 2 == 0 { lo.y } else { hi.y },
                if i & 4 == 0 { lo.z } else { hi.z },
            );
            *view_proj * p.extend(1.0)
        })
        .collect();
    let outside = |f: &dyn Fn(&glam::Vec4) -> bool| corners.iter().all(f);
    !(outside(&|c| c.x < -c.w)
        || outside(&|c| c.x > c.w)
        || outside(&|c| c.y < -c.w)
        || outside(&|c| c.y > c.w)
        || outside(&|c| c.z < 0.0)
        || outside(&|c| c.z > c.w))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rain() -> Weather {
        Weather {
            material: Some("Environment\\RainMaterial.rsc:resource".into()),
            count: 625,
            direction: [0.4, 0.3, -1.0],
            speed: [30.0, 10.0],
            flag: true,
            floats_44: [0.02, 10.0, 16.0, 20.0, 2.0, 1.0],
            fade_time: 20.0,
            colors: [0; 3],
            floats_6c: [0.0; 5],
        }
    }

    #[test]
    fn drops_fall_and_stay_in_their_cube() {
        let mut p = Precipitation::new(rain(), 1);
        assert_eq!(p.drops.len(), 625);
        let before: f32 = p.drops.iter().map(|d| d.pos.z).sum();
        p.update(0.01);
        let after: f32 = p.drops.iter().map(|d| d.pos.z).sum();
        assert!(after < before);
        for _ in 0..200 {
            p.update(1.0 / 30.0);
        }
        assert!(p
            .drops
            .iter()
            .all(|d| d.pos.abs().max_element() <= 5.0 + 1e-4));
        // A camera at the origin facing −z.
        let vp = crate::view::scene::projection(40.0, 1.6, 0.5, 500.0);
        let b = p.batch(0.5).unwrap();
        let t = tiles(b.size, Vec3::ZERO, &vp);
        assert!(!t.is_empty() && t.len() < 125);
        assert_eq!(b.vertices.len(), 625 * 32);
        assert!(p.batch(0.0).is_none());
    }
}
