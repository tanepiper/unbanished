//! Placing hills and stamping their shapes into the height field.

use super::*;

impl Generator {
    /// `FUN_1400d2930`: scatter exclusion circles and hill circles, group overlapping hills, then
    /// join every group to the first with a bridge of radius-10 circles. Results go to `vec28`.
    pub(super) fn place_hills(&mut self) {
        let r = self.record;
        let edge = ((r.at(0) >> 1) - 2) as f32;
        let scatter = |rng: &mut unbanished_core::random::Random, lo: i32, hi: i32| -> [f32; 3] {
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
    pub(super) fn stamp(
        &self,
        aux: &mut [f32],
        at: [f32; 2],
        r: f32,
        base: f32,
        value: f32,
        raise: bool,
    ) {
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
}
