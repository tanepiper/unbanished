//! The five generation stages. Each function names the original it ports.

use super::{Generator, PathPoint};
use banished_core::noise::PerlinNoise;

/// `Math::Vector2::Normalize` (Runtime 0x18006d0d0): multiply by `1 / length`; zero stays zero.
fn normalize(v: [f32; 2]) -> [f32; 2] {
    let len = sqrtf(v[0] * v[0] + v[1] * v[1]);
    if len == 0.0 {
        return v;
    }
    let inv = 1.0 / len;
    [v[0] * inv, v[1] * inv]
}

/// `FUN_1400d43b0` grows its stamp until the logistic weight reaches this (`DAT_140178844`).
const STAMP_CUTOFF: f32 = 0.99;
/// Steepness of lake shaping stamps (`DAT_14017887c`).
const LAKE_STEEPNESS: f32 = 2.25;
/// Steepness of hill mask stamps (`DAT_140178864`).
const HILL_STEEPNESS: f32 = 1.75;

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

    /// Stage 3: `FUN_1400d2930` (hill circles) then `FUN_1400d3de0` (rasterise records).
    pub fn stage3_shape(&mut self) {
        self.place_hills();
        self.rasterise_records();
    }

    /// `FUN_1400d2930`: scatter exclusion circles and hill circles, group overlapping hills, then
    /// join every group to the first with a bridge of radius-10 circles. Results go to `vec28`.
    fn place_hills(&mut self) {
        let r = self.record;
        let edge = ((r.at(0) >> 1) - 2) as f32;
        let scatter = |rng: &mut banished_core::random::Random, lo: i32, hi: i32| -> [f32; 3] {
            let radius = rng.rand_float() * (hi - lo) as f32 + lo as f32;
            let a = rng.rand_float_tol();
            let b = rng.rand_float_tol();
            let span = edge - (radius as i32) as f32;
            [span * b, span * a, radius]
        };
        // Exclusion zones: hills may not sit inside them.
        let mut exclusions: Vec<[f32; 3]> = Vec::new();
        for _ in 0..r.at(0x10) {
            exclusions.push(scatter(&mut self.rng, r.at(0x14), r.at(0x18)));
        }
        for _ in 0..r.at(0x04) {
            let c = scatter(&mut self.rng, r.at(0x08), r.at(0x0c));
            let blocked = exclusions.iter().any(|e| {
                let d = [c[0] - e[0], c[1] - e[1]];
                e[2] > sqrtf(d[1] * d[1] + d[0] * d[0]) + c[2]
            });
            if !blocked {
                self.vec28.push(c);
            }
        }

        // Union-find by minimum label over overlapping circles, in creation order.
        let n = self.vec28.len();
        let mut labels = vec![-1i32; n];
        if n == 0 {
            return;
        }
        labels[0] = 0;
        let mut next = 0i32;
        for i in 0..n {
            let mut cur = next;
            labels[i] = cur;
            let mut merge = std::collections::BTreeSet::from([cur]);
            for j in 0..i {
                let (a, b) = (self.vec28[i], self.vec28[j]);
                let d = [a[0] - b[0], a[1] - b[1]];
                let dist = sqrtf(d[1] * d[1] + d[0] * d[0]);
                if a[2] + b[2] > dist * 1.2 {
                    merge.insert(labels[j]);
                    if labels[j] < cur {
                        cur = labels[j];
                    }
                }
            }
            let mut max = 0;
            for l in labels.iter_mut().take(i + 1) {
                if max < *l {
                    max = *l;
                }
                if merge.contains(l) {
                    *l = cur;
                }
            }
            next = max + 1;
        }

        // Members of each group, groups in ascending label order.
        let unique: std::collections::BTreeSet<i32> = labels.iter().copied().collect();
        let mut groups: Vec<Vec<usize>> = unique
            .iter()
            .map(|&l| (0..n).filter(|&c| labels[c] == l).collect())
            .collect();

        // Bridge every later group to its nearest member of group 0, then fold it into group 0.
        for g in 1..groups.len() {
            let a = groups[g][self.rng.rand_below(groups[g].len() as u32) as usize];
            let mut b = groups[0][self.rng.rand_below(groups[0].len() as u32) as usize];
            let mut best = 1_000_000.0f32;
            for &c in &groups[0] {
                let d = [
                    self.vec28[a][0] - self.vec28[c][0],
                    self.vec28[a][1] - self.vec28[c][1],
                ];
                let len = sqrtf(d[0] * d[0] + d[1] * d[1]);
                if best > len {
                    best = len;
                    b = c;
                }
            }
            let delta = [
                self.vec28[b][0] - self.vec28[a][0],
                self.vec28[b][1] - self.vec28[a][1],
            ];
            let length = sqrtf(delta[0] * delta[0] + delta[1] * delta[1]);
            let dir = normalize(delta);
            let mut cur = [self.vec28[a][0], self.vec28[a][1]];
            let mut t = 0.0f32;
            while length > t {
                self.vec28.push([cur[0], cur[1], 10.0]);
                t += 2.0;
                cur = [cur[0] + dir[0] * 2.0, cur[1] + dir[1] * 2.0];
            }
            let moved = std::mem::take(&mut groups[g]);
            groups[0].extend(moved);
        }
    }

    /// `FUN_1400d43b0`: stamp a logistic disc of radius `r` (steepness `base`) centred on `at`
    /// into `aux`, keeping the minimum of `value * w` (or the maximum of `value * (1 - w)` when
    /// `raise`), where `w = 1 / (base^(r - d) + 1)`.
    fn stamp(&self, aux: &mut [f32], at: [f32; 2], r: f32, base: f32, value: f32, raise: bool) {
        let mut reach = 1.0f32;
        let mut w = 1.0 / (powf(base, -(1.0 - r)) + 1.0);
        while STAMP_CUTOFF > w {
            reach += 1.0;
            w = 1.0 / (powf(base, -(reach - r)) + 1.0);
        }
        let width = self.size + 1;
        let half = self.size >> 1;
        let (cx, cy) = (at[0] as i32 + half, at[1] as i32 + half);
        let ext = reach as i32 + 1;
        for dy in -ext..=ext {
            let row = cy + dy;
            if row < 0 || row >= width {
                continue;
            }
            for dx in -ext..=ext {
                let col = cx + dx;
                if col < 0 || col >= width {
                    continue;
                }
                let d = sqrtf((dx * dx + dy * dy) as f32);
                let w = 1.0 / (powf(base, -(d - r)) + 1.0);
                let cell = &mut aux[(row * width + col) as usize];
                if raise {
                    let v = (1.0 - w) * value;
                    if v > *cell {
                        *cell = v;
                    }
                } else {
                    let v = w * value;
                    if *cell > v {
                        *cell = v;
                    }
                }
            }
        }
    }

    /// `FUN_1400d3de0`: carve rivers, lakes and hill masks into the grid:
    /// `grid = carve * grid * hills - raise`.
    fn rasterise_records(&mut self) {
        let n = self.grid.len();
        let (mut carve, mut raise, mut hills) = (vec![1.0f32; n], vec![0.0f32; n], vec![1.0f32; n]);
        for path in &self.paths {
            for p in path {
                let [depth, r1, b1, r2, b2] = p.params;
                self.stamp(&mut carve, p.pos, r1, b1, 1.0, false);
                self.stamp(&mut raise, p.pos, r2, b2, depth, true);
            }
        }
        for p in &self.points {
            self.stamp(&mut raise, [p[0], p[1]], p[2], LAKE_STEEPNESS, 2.0, true);
        }
        for c in &self.vec28 {
            self.stamp(&mut hills, [c[0], c[1]], c[2], HILL_STEEPNESS, 1.0, false);
        }
        for i in 0..n {
            self.grid[i] = carve[i] * self.grid[i] * hills[i] - raise[i];
        }
    }

    /// Stage 4, `FUN_1400d1550`: `record[+0x24]` walks from random interior cells, each moving
    /// to the lowest 3x3 neighbour and exchanging a fraction of the height difference.
    pub fn stage4_downhill(&mut self) {
        let w = self.size + 1;
        for _ in 0..self.record.at(0x24) {
            let mut x = self.rng.rand_below((w - 2) as u32) as i32 + 1;
            let mut y = self.rng.rand_below((w - 2) as u32) as i32 + 1;
            loop {
                let idx = y * w + x;
                let h = self.grid[idx as usize];
                if 0.0 >= h {
                    break;
                }
                let (mut low, mut low_idx, mut lx, mut ly) = (h, idx, -1, -1);
                for ny in [y - 1, y, y + 1] {
                    for nx in [x - 1, x, x + 1] {
                        let v = self.grid[(ny * w + nx) as usize];
                        if low > v {
                            (low, low_idx, lx, ly) = (v, ny * w + nx, nx, ny);
                        }
                    }
                }
                let mut d = (h - low) * 0.0625;
                if d > 0.8 {
                    d = 0.8;
                }
                self.grid[idx as usize] = (1.0 - d) * h + d * low;
                self.grid[low_idx as usize] = (1.0 - d) * low + d * h;
                if low_idx == idx || lx < 1 || ly < 1 || lx >= w - 1 || ly >= w - 1 {
                    break;
                }
                (x, y) = (lx, ly);
            }
        }
    }

    /// Stage 5: `FUN_1400d4120` (carve rivers and lakes again, no hill mask) then
    /// `FUN_1400d1820` twice.
    pub fn stage5_smooth(&mut self) {
        let n = self.grid.len();
        let (mut carve, mut raise) = (vec![1.0f32; n], vec![0.0f32; n]);
        for path in &self.paths {
            for p in path {
                let [depth, r1, b1, r2, b2] = p.params;
                self.stamp(&mut carve, p.pos, r1, b1, 1.0, false);
                self.stamp(&mut raise, p.pos, r2, b2, depth, true);
            }
        }
        for p in &self.points {
            self.stamp(&mut raise, [p[0], p[1]], p[2], LAKE_STEEPNESS, 2.0, true);
        }
        for i in 0..n {
            self.grid[i] = carve[i] * self.grid[i] - raise[i];
        }
        self.blur();
        self.blur();
    }

    /// `FUN_1400d1820`: `(2 * centre + (x-1, y-1) + (x-1, y+1)) / 4` with clamped reads, written
    /// for `x, y < size`. The temporary buffer's last row and column are never written; the
    /// original leaves them as whatever its allocator returned, taken here as zero.
    fn blur(&mut self) {
        let w = self.size + 1;
        let clamp = |v: i32| v.clamp(0, w - 1);
        let mut out = vec![0.0f32; self.grid.len()];
        for y in 0..self.size {
            for x in 0..self.size {
                let at = |gx: i32, gy: i32| self.grid[(clamp(gy) * w + clamp(gx)) as usize];
                let mut sum = at(x, y) * 2.0;
                let mut weight = 2.0f32;
                for dy in [-1, 1] {
                    sum += at(x - 1, y + dy);
                    weight += 1.0;
                }
                out[(y * w + x) as usize] = sum / weight;
            }
        }
        self.grid = out;
    }
}
