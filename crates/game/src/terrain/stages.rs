//! The five generation stages. Each function names the original it ports.

use super::{Generator, PathPoint};
use crate::noise::PerlinNoise;

/// `Math::Vector2::Normalize` (Runtime 0x18006d0d0): multiply by `1 / length`; zero stays zero.
fn normalize(v: [f32; 2]) -> [f32; 2] {
    let len = sqrtf(v[0] * v[0] + v[1] * v[1]);
    if len == 0.0 {
        return v;
    }
    let inv = 1.0 / len;
    [v[0] * inv, v[1] * inv]
}

/// Parameters of the first river path (`FUN_1400d2560`).
const FIRST_PATH: [f32; 5] = [2.0, 10.0, 1.25, 4.0, 4.0];
/// Parameters of branch paths (`FUN_1400d2430`).
const BRANCH_PATH: [f32; 5] = [1.5, 8.0, 1.25, 1.5, 16.0];

/// Segment intersection test used three times by `FUN_1400d1d20`. Returns true when the open
/// segments cross. Operand order follows the disassembly.
fn crosses(p0: [f32; 2], p1: [f32; 2], q0: [f32; 2], q1: [f32; 2]) -> bool {
    let (qdy, qdx) = (q1[1] - q0[1], q1[0] - q0[0]);
    let (pdy, pdx) = (p1[1] - p0[1], p1[0] - p0[0]);
    let (oy, ox) = (p0[1] - q0[1], p0[0] - q0[0]);
    let den = pdx * qdy - qdx * pdy;
    if den == 0.0 {
        return false;
    }
    let ua = (qdx * oy - ox * qdy) / den;
    let ub = (pdx * oy - ox * pdy) / den;
    ua > 0.0 && 1.0 > ua && ub > 0.0 && 1.0 > ub
}

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

    /// Stage 2: `FUN_1400d2560` (first path), `FUN_1400d2620` (lake points), `FUN_1400d2430`
    /// (branch paths).
    pub fn stage2_records(&mut self) {
        while self.paths.is_empty() {
            self.build_path(self.size + 1, self.size as f32, FIRST_PATH, -1);
        }
        self.place_lakes();
        for _ in 0..self.record.at(0x20) {
            for _ in 0..1000 {
                let idx = self.rng.rand_below(self.paths.len() as u32) as i32;
                if self.build_path(self.size + 1, self.size as f32, BRANCH_PATH, idx) {
                    break;
                }
            }
        }
    }

    /// `FUN_1400d19e0`: recursive midpoint displacement between `i0` and `i1`.
    fn displace(&mut self, pts: &mut [PathPoint], i0: usize, i1: usize, bound: f32, scale: f32) {
        if i1 - i0 < 2 {
            return;
        }
        let (a, b) = (pts[i0].pos, pts[i1].pos);
        let mid = [(b[0] + a[0]) * 0.5, (b[1] + a[1]) * 0.5];
        let d = [b[0] - a[0], b[1] - a[1]];
        let n = normalize([-d[1], d[0]]);
        let lo = -bound;
        let p = loop {
            let r = self.rng.rand_float_tol() * scale * 0.666;
            let p = [n[0] * r + mid[0], n[1] * r + mid[1]];
            let u1 = normalize([b[0] - p[0], b[1] - p[1]]);
            let u2 = normalize([a[0] - p[0], a[1] - p[1]]);
            let dot = u2[1] * u1[1] + u1[0] * u2[0];
            if lo > p[0] || (!(p[0] > lo) && lo > p[1]) {
                continue;
            }
            if p[0] > bound || (!(bound > p[0]) && p[1] > bound) {
                continue;
            }
            if dot > 0.0 {
                continue;
            }
            break p;
        };
        let scale = scale * 0.5;
        let m = ((i1 - i0) >> 1) + i0;
        pts[m].pos = p;
        self.displace(pts, i0, m, bound, scale);
        self.displace(pts, m, i1, bound, scale);
    }

    /// `FUN_1400d1d20`: build a candidate path of `count` points; append it unless it is
    /// degenerate or crosses itself or an existing path. `from < 0` starts a new river at the map
    /// edge, otherwise it branches from `paths[from]`.
    fn build_path(&mut self, count: i32, extent: f32, params: [f32; 5], from: i32) -> bool {
        let count = count as usize;
        let mut pts = vec![
            PathPoint {
                pos: [0.0, 0.0],
                params
            };
            count
        ];
        let last = count - 1;
        if from < 0 {
            pts[0].pos = [0.0, extent * -0.5];
            match self.rng.rand_below(3) {
                0 => pts[last].pos = [0.0, extent * 0.5],
                1 => pts[last].pos = [extent * 0.5, 0.0],
                2 => pts[last].pos = [extent * -0.5, 0.0],
                _ => {}
            }
            let a = self.rng.rand_float_tol();
            let b = self.rng.rand_float_tol();
            pts[count >> 1].pos = [b * extent * 0.125, a * extent * 0.125];
            self.displace(&mut pts, 0, count >> 1, extent * 0.5, extent * 0.25);
            self.displace(&mut pts, count >> 1, last, extent * 0.5, extent * 0.25);
        } else {
            let base = &self.paths[from as usize];
            let q = base.len() as i32 / 4;
            let k = self.rng.rand_range(q, q * 3);
            pts[0].pos = self.paths[from as usize][k as usize].pos;
            let v = self.rng.rand_float_tol() * extent * 0.5;
            match self.rng.rand_below(4) {
                0 => pts[last].pos = [v, extent * 0.5],
                1 => pts[last].pos = [extent * 0.5, v],
                2 => pts[last].pos = [extent * -0.5, v],
                3 => pts[last].pos = [v, extent * -0.5],
                _ => {}
            }
            self.displace(&mut pts, 0, last, extent * 0.5, extent * 0.25);
        }

        // Each point's nearest other point must be a neighbour along the path.
        for i in 0..count {
            let (mut best, mut best_j) = (1_000_000.0f32, -2i64);
            for (j, q) in pts.iter().enumerate() {
                if i == j {
                    continue;
                }
                let d = [pts[i].pos[0] - q.pos[0], pts[i].pos[1] - q.pos[1]];
                let ls = d[0] * d[0] + d[1] * d[1];
                if best > ls {
                    best = ls;
                    best_j = j as i64;
                }
            }
            if best_j != i as i64 + 1 && best_j != i as i64 - 1 {
                return false;
            }
        }
        // No crossings with itself or with existing paths.
        for i in 0..last {
            let (p0, p1) = (pts[i].pos, pts[i + 1].pos);
            for j in 0..last {
                if j == i || j + 1 == i || j == i + 1 {
                    continue;
                }
                if crosses(p0, p1, pts[j].pos, pts[j + 1].pos) {
                    return false;
                }
            }
            for path in &self.paths {
                for c in 0..path.len().saturating_sub(1) {
                    if crosses(p0, p1, path[c].pos, path[c + 1].pos) {
                        return false;
                    }
                }
            }
        }
        self.paths.push(pts);
        true
    }

    /// `FUN_1400d2620`: lake centres (`lakes`) and the points shaping each lake (`points`).
    fn place_lakes(&mut self) {
        let r = self.record;
        let count = self.rng.rand_below((r.at(0x2c) - r.at(0x28) + 1) as u32) as i32 + r.at(0x28);
        let depth = self.rng.rand_below((r.at(0x34) - r.at(0x30)) as u32) as i32 + r.at(0x30);
        let depth_f = depth as f32;
        let floor = (depth - r.at(0x40)) as f32;
        let span = ((r.at(0) >> 1) - 2) as f32 - depth_f;
        let (rmin, rspan) = (r.at(0x3c) as f32, (r.at(0x40) - r.at(0x3c)) as f32);
        for _ in 0..count {
            let a = self.rng.rand_float_tol();
            let b = self.rng.rand_float_tol();
            let (y, x) = (span * a, b * span);
            for _ in 0..r.at(0x38) {
                let radius = self.rng.rand_float() * rspan + rmin;
                let spread = self.rng.rand_float() * 0.5 + 0.5;
                let dy = self.rng.rand_float_tol();
                let reach = depth_f - radius;
                let dx = self.rng.rand_float_tol();
                let v = normalize([dx, dy]);
                self.points
                    .push([reach * v[0] * spread + x, reach * v[1] * spread + y, radius]);
            }
            self.points.push([x, y, floor]);
            self.lakes.push([x, y, depth_f]);
        }
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
