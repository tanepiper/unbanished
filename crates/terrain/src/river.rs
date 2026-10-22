//! The river strip builder: a generated path record becomes the triangle strip the map draws with
//! the River material. See docs/TERRAIN-WATER-GEOMETRY.md.
//!
//! 1. The path's segments, each end moved by `(+0.5, −0.5)`, go in a spatial index (4-unit cells).
//!    Each of them but the last puts bank points every quarter unit at the river's half width (the
//!    first point's `params[3]` + 2) on both sides: left into bank A, right into bank B, each kept
//!    only when no segment is nearer than the half width. The joint to the next segment adds an arc
//!    on its outer side, 20 steps per right angle.
//! 2. Each path point gets the average backward tangent of its two segments.
//! 3. For each path point and four directions between its tangent and the next one, the nearest
//!    bank point of either bank to the path point (within twice the half width) is taken. When both
//!    banks moved on and the line between them is square to the direction, the bank stretches
//!    walked since the last row are resampled to the same count and added as rows, with a `v`
//!    running along the river at 1/32 of the midline's length.
//! 4. Each pair of rows becomes four quads across, `u` 0 to 1 from bank A to bank B; equal
//!    vertices are shared.
//!
//! The arithmetic follows the original's float order so the mesh matches it bit for bit.
//!
//! Original: `FUN_1400d4fd0`, `FUN_1400d0f00`, `FUN_1400d12c0`, `FUN_1400d0c20`, `FUN_1400d0830`.

mod water;
pub use water::{water_mesh, WaterMesh};

use std::collections::{BTreeMap, HashMap};

use super::PathPoint;

/// Cells of the spatial indices: `0x40800000` (4) and its inverse.
const CELL_INV: f32 = 0.25;

/// `(int)` truncation as `cvttss2si`.
fn cell(v: f32) -> i32 {
    (v * CELL_INV) as i32
}

fn length(v: [f32; 2]) -> f32 {
    (v[0] * v[0] + v[1] * v[1]).sqrt()
}

/// `Vector2::Dot`: `other.y × this.y + other.x × this.x`.
fn dot(a: [f32; 2], b: [f32; 2]) -> f32 {
    b[1] * a[1] + b[0] * a[0]
}

/// `Vector2::Normalize`: the vector times `1 / length`, unchanged when zero.
fn normalize(v: [f32; 2]) -> [f32; 2] {
    let l = length(v);
    if l == 0.0 {
        return v;
    }
    let inv = 1.0 / l;
    [v[0] * inv, v[1] * inv]
}

fn sub(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

/// A segment record (`0x38` bytes; `+0x14` a, `+0x1c` b, `+0x24` direction, `+0x2c` length).
struct Segment {
    a: [f32; 2],
    b: [f32; 2],
    d: [f32; 2],
    len: f32,
    perp: [f32; 2],
    /// Registered cells, inclusive.
    lo: (i32, i32),
    hi: (i32, i32),
}

/// `FUN_1400d0f00`: no segment registered in the cells around `p` (± 1.1 × `w`) is nearer than
/// `w`. The projection test adds the parameter to both coordinates of `a + d`, as the
/// original does.
fn clear_of(segments: &[Segment], p: [f32; 2], w: f32) -> bool {
    let r = w * 1.1;
    let lo = (cell(p[0] - r), cell(p[1] - r));
    let hi = (cell(p[0] + r), cell(p[1] + r));
    for s in segments {
        if s.hi.0 < lo.0 || hi.0 < s.lo.0 || s.hi.1 < lo.1 || hi.1 < s.lo.1 {
            continue;
        }
        let da = sub(p, s.a);
        if w > dot(da, da).sqrt() {
            return false;
        }
        let db = sub(p, s.b);
        if w > dot(db, db).sqrt() {
            return false;
        }
        let t = dot(da, s.d);
        if 0.0 < t && t < s.len {
            let q = [t + (s.a[0] + s.d[0]), t + (s.a[1] + s.d[1])];
            let e = sub(q, p);
            if dot(e, e).sqrt() < w {
                return false;
            }
        }
    }
    true
}

/// A bank: its points in order and their cells (each point in the cell of its position).
#[derive(Default)]
struct Bank {
    points: Vec<[f32; 2]>,
    cells: HashMap<(i32, i32), Vec<usize>>,
}

impl Bank {
    fn add(&mut self, p: [f32; 2]) {
        let i = self.points.len();
        self.points.push(p);
        self.cells
            .entry((cell(p[0]), cell(p[1])))
            .or_default()
            .push(i);
    }

    /// `FUN_1400d12c0`: the nearest point to `p` among the cells within `2w`, its index
    /// written to `index` (kept when there is none) and the point returned (`p` when none).
    /// Cells go by rows; a cell's points newest first; the first nearest wins.
    fn nearest(&self, p: [f32; 2], w: f32, index: &mut usize) -> [f32; 2] {
        let r = w * 2.0;
        let lo = (cell(p[0] - r), cell(p[1] - r));
        let hi = (cell(p[0] + r), cell(p[1] + r));
        let mut best = 1_000_000.0f32;
        let mut out = p;
        for y in lo.1..=hi.1 {
            for x in lo.0..=hi.0 {
                let Some(list) = self.cells.get(&(x, y)) else {
                    continue;
                };
                for &i in list.iter().rev() {
                    let d = length(sub(p, self.points[i]));
                    if d < best {
                        best = d;
                        *index = i;
                        out = self.points[i];
                    }
                }
            }
        }
        out
    }

    /// `FUN_1400d0c20`: `count` points evenly along the bank from point `start`, `total` long.
    fn resample(&self, start: usize, count: usize, total: f32) -> Vec<[f32; 2]> {
        let pts = &self.points;
        let step = total / count as f32;
        let mut seg = start;
        let mut seg_len = length(sub(pts[seg], pts[seg + 1]));
        let mut end = seg_len;
        let mut pos = 0.0f32;
        let mut out = Vec::with_capacity(count);
        while out.len() < count {
            if pos < end {
                let f = 1.0 - (end - pos) / seg_len;
                let g = 1.0 - f;
                out.push([
                    g * pts[seg][0] + f * pts[seg + 1][0],
                    g * pts[seg][1] + f * pts[seg + 1][1],
                ]);
                pos += step;
            } else {
                seg += 1;
                if pts.len() <= seg + 1 {
                    break;
                }
                seg_len = length(sub(pts[seg], pts[seg + 1]));
                end += seg_len;
            }
        }
        out
    }

    /// `FUN_1400d0830`: as [`Bank::resample`], each point with its `v`:
    /// `(walked / total) × v_step + v_start`.
    fn rows(
        &self,
        start: usize,
        count: usize,
        total: f32,
        v_step: f32,
        v_start: f32,
        out: &mut Vec<([f32; 2], f32)>,
    ) {
        let pts = &self.points;
        let step = total / count as f32;
        let mut seg = start;
        let mut seg_len = length(sub(pts[seg], pts[seg + 1]));
        let mut end = seg_len;
        let mut pos = 0.0f32;
        let mut n = 0;
        while n < count {
            if pos < end {
                let f = 1.0 - (end - pos) / seg_len;
                let g = 1.0 - f;
                let p = [
                    g * pts[seg][0] + f * pts[seg + 1][0],
                    g * pts[seg][1] + f * pts[seg + 1][1],
                ];
                let v = ((end - g * seg_len) / total) * v_step + v_start;
                out.push((p, v));
                pos += step;
                n += 1;
            } else {
                seg += 1;
                if pts.len() <= seg + 1 {
                    break;
                }
                let d = sub(pts[seg], pts[seg + 1]);
                seg_len = dot(d, d).sqrt();
                end += seg_len;
            }
        }
    }

    /// The bank's length from point `from` to point `to` (exclusive of `to`'s next).
    fn walked(&self, from: usize, to: usize) -> f32 {
        let mut sum = 0.0f32;
        for m in from..to.saturating_sub(1) {
            let d = sub(self.points[m], self.points[m + 1]);
            sum += dot(d, d).sqrt();
        }
        sum
    }

    fn walked_length(&self, from: usize, to: usize) -> f32 {
        let mut sum = 0.0f32;
        for m in from..to.saturating_sub(1) {
            sum += length(sub(self.points[m], self.points[m + 1]));
        }
        sum
    }
}

/// The strip of one path: vertices `(x, y, u, v)` and triangle indices.
///
/// Original: `FUN_1400d4fd0`.
pub fn river_strip(path: &[PathPoint]) -> (Vec<[f32; 4]>, Vec<u16>) {
    let n = path.len();
    if n < 2 {
        return (Vec::new(), Vec::new());
    }
    // 1. Segments.
    let segments: Vec<Segment> = path
        .windows(2)
        .map(|w| {
            let a = [w[0].pos[0] + 0.5, w[0].pos[1] - 0.5];
            let b = [w[1].pos[0] + 0.5, w[1].pos[1] - 0.5];
            let diff = sub(b, a);
            let len = length(diff);
            let d = normalize(diff);
            let (min, max) = (
                [a[0].min(b[0]), a[1].min(b[1])],
                [a[0].max(b[0]), a[1].max(b[1])],
            );
            Segment {
                a,
                b,
                d,
                len,
                perp: [-d[1], d[0]],
                lo: (cell(min[0]), cell(min[1])),
                hi: (cell(max[0]), cell(max[1])),
            }
        })
        .collect();
    let w = path[0].params[3] + 2.0;
    let (mut bank_a, mut bank_b) = (Bank::default(), Bank::default());
    for i in 0..n.saturating_sub(2) {
        let s = &segments[i];
        let next = &segments[i + 1];
        let steps = ((s.len * 4.0) as i32).max(1);
        let (dxl, dyl) = (s.d[0] * s.len, s.d[1] * s.len);
        let (px, py) = (s.perp[0] * w, s.perp[1] * w);
        for k in 0..=steps {
            let t = k as f32 / steps as f32;
            let p = [s.a[0] + dxl * t, dyl * t + s.a[1]];
            let left = [p[0] - px, p[1] - py];
            if clear_of(&segments, left, w) {
                bank_a.add(left);
            }
            let right = [px + p[0], py + p[1]];
            if clear_of(&segments, right, w) {
                bank_b.add(right);
            }
        }
        // The joint: an arc on the outer side.
        let cross = next.d[1] * s.d[0] - next.d[0] * s.d[1];
        let left_turn = cross > 0.0;
        let sign = if left_turn { -1.0f32 } else { 1.0 };
        let cos = next.d[0] * s.d[0] + next.d[1] * s.d[1];
        let arc = ((unbanished_core::crt::acosf(cos) * 57.29578 / 90.0 * 20.0) as i32).max(1);
        for k in 0..arc {
            let t = k as f32 / arc as f32;
            let u = 1.0 - t;
            let p = normalize([
                u * s.perp[0] + next.perp[0] * t,
                u * s.perp[1] + next.perp[1] * t,
            ]);
            let q = [s.b[0] + p[0] * w * sign, s.b[1] + sign * (p[1] * w)];
            if clear_of(&segments, q, w) {
                if left_turn {
                    bank_a.add(q);
                } else {
                    bank_b.add(q);
                }
            }
        }
    }
    // 2. Tangents.
    let tangents: Vec<[f32; 2]> = (0..n)
        .map(|i| {
            let back = if 0 < i {
                normalize(sub(path[i - 1].pos, path[i].pos))
            } else {
                [0.0, 0.0]
            };
            let ahead = if i + 1 < n {
                normalize(sub(path[i].pos, path[i + 1].pos))
            } else {
                [0.0, 0.0]
            };
            normalize([ahead[0] + back[0], ahead[1] + back[1]])
        })
        .collect();
    // 3. Rows.
    let mut rows_a: Vec<([f32; 2], f32)> = Vec::new();
    let mut rows_b: Vec<([f32; 2], f32)> = Vec::new();
    let (mut ia, mut ib) = (0usize, 0usize);
    let (mut ja, mut jb) = (0usize, 0usize);
    let mut v_acc = -16.0f32;
    let mut prev_mid = [0.0f32; 2];
    let mut first = true;
    for i in 0..n - 1 {
        for k in 0..4 {
            let t = k as f32 * 0.25;
            let u = 1.0 - t;
            let dir = normalize([
                u * tangents[i][0] + t * tangents[i + 1][0],
                u * tangents[i][1] + t * tangents[i + 1][1],
            ]);
            let pa = bank_a.nearest(path[i].pos, w, &mut ja);
            let pb = bank_b.nearest(path[i].pos, w, &mut jb);
            if ja <= ia || jb <= ib || (ja == 0 && jb == 0) {
                continue;
            }
            if dot(normalize(sub(pa, pb)), dir).abs() >= 0.02 {
                continue;
            }
            let len_a = bank_a.walked(ia, ja);
            let len_b = bank_b.walked_length(ib, jb);
            let most = if len_b < len_a { len_a } else { len_b };
            if len_a < 2.0 && len_b < 2.0 {
                continue;
            }
            let count = (most as i32).max(1);
            let (step_a, step_b) = (len_a / count as f32, len_b / count as f32);
            if step_a < 0.001 || step_b < 0.001 {
                continue;
            }
            let count = count as usize;
            let a = bank_a.resample(ia, count, len_a);
            let b = bank_b.resample(ib, count, len_b);
            let mut mid_len = 0.0f32;
            for m in 0..count - 1 {
                let next = [
                    0.5 * (a[m + 1][0] + b[m + 1][0]),
                    0.5 * (a[m + 1][1] + b[m + 1][1]),
                ];
                let cur = [0.5 * (a[m][0] + b[m][0]), 0.5 * (a[m][1] + b[m][1])];
                mid_len += length(sub(cur, next));
            }
            if !first {
                let start = [0.5 * (b[0][0] + a[0][0]), 0.5 * (b[0][1] + a[0][1])];
                v_acc += length(sub(prev_mid, start)) / 32.0;
            }
            let last = count - 1;
            prev_mid = [
                0.5 * (b[last][0] + a[last][0]),
                0.5 * (a[last][1] + b[last][1]),
            ];
            first = false;
            let v_step = mid_len / 32.0;
            bank_a.rows(ia, count, len_a, v_step, v_acc, &mut rows_a);
            bank_b.rows(ib, count, len_b, v_step, v_acc, &mut rows_b);
            v_acc = rows_a.last().map_or(v_acc, |r| r.1);
            ia = ja;
            ib = jb;
        }
    }
    // 4. Quads.
    let mut vertices: Vec<[f32; 4]> = Vec::new();
    let mut seen: BTreeMap<[u32; 4], u16> = BTreeMap::new();
    let mut index = |v: [f32; 4], vertices: &mut Vec<[f32; 4]>| -> u16 {
        // Equal floats share a vertex (0 and −0 too).
        let key = v.map(|f| if f == 0.0 { 0 } else { f.to_bits() });
        *seen.entry(key).or_insert_with(|| {
            vertices.push(v);
            (vertices.len() - 1) as u16
        })
    };
    let mut indices = Vec::new();
    for r in 0..rows_a.len().saturating_sub(1) {
        let (a0, va0) = rows_a[r];
        let (a1, va1) = rows_a[r + 1];
        let (b0, vb0) = rows_b[r];
        let (b1, vb1) = rows_b[r + 1];
        let lerp =
            |p: [f32; 2], q: [f32; 2], t: f32| [p[0] + t * (q[0] - p[0]), p[1] + (q[1] - p[1]) * t];
        let (mut u0, mut u1) = (0.0f32, 0.25f32);
        let (mut p0, mut p1, mut s0, mut s1) = (a0, a1, va0, va1);
        for _ in 0..4 {
            let q0 = lerp(a0, b0, u1);
            let q1 = lerp(a1, b1, u1);
            let t0 = va0 + (vb0 - va0) * u1;
            let t1 = va1 + (vb1 - va1) * u1;
            let ia = index([p0[0], p0[1], u0, s0], &mut vertices);
            let ib = index([q0[0], q0[1], u1, t0], &mut vertices);
            let ic = index([q1[0], q1[1], u1, t1], &mut vertices);
            let id = index([p1[0], p1[1], u0, s1], &mut vertices);
            indices.extend_from_slice(&[ia, ic, ib, ia, id, ic]);
            u0 = u1;
            u1 += 0.25;
            (p0, p1, s0, s1) = (q0, q1, t0, t1);
        }
    }
    (vertices, indices)
}
