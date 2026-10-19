//! Gameplay queries on the saved height grid: the vertical ray cast that gives the ground height
//! and normal anywhere on the map (`FUN_1400b6460` → `FUN_1400afef0` → `FUN_1400afce0`), and the
//! natural cell type derived from it (`FUN_1400a3790`).
//!
//! Positions are map-centred: cell `(i, j)` covers `[i - size/2, i - size/2 + 1)` and likewise
//! for `y`. Float operations follow the disassembly so results are bit-exact.

use super::mesh::HeightGrid;

type Vec3 = [f32; 3];

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// `Math::Vector3::Cross`.
fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        b[0] * a[2] - a[0] * b[2],
        a[0] * b[1] - b[0] * a[1],
    ]
}

fn dot(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// `Math::Vector3::Normalize` (by value): `v * (1 / |v|)`, unchanged when zero.
fn normalize(v: Vec3) -> Vec3 {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len == 0.0 {
        return v;
    }
    let inv = 1.0 / len;
    [v[0] * inv, v[1] * inv, v[2] * inv]
}

const EPSILON: f32 = -1e-4;
const RAY_TOP: f32 = 100.0;
const RAY_LENGTH: f32 = 200.0;

/// `FUN_1400afce0`: ray/triangle test. `t` is written once the barycentric tests pass, even when
/// it is then rejected by the range test, as in the original.
#[allow(clippy::too_many_arguments, clippy::neg_cmp_op_on_partial_ord)]
fn ray_triangle(
    origin: Vec3,
    dir: Vec3,
    tmax: f32,
    v0: Vec3,
    v1: Vec3,
    v2: Vec3,
    t: &mut f32,
    normal: &mut Vec3,
) -> bool {
    let e1 = sub(v1, v0);
    let e2 = sub(v2, v0);
    let p = cross(dir, e2);
    let det = dot(e1, p);
    let s = sub(origin, v0);
    let q = cross(s, e1);
    let u = dot(s, p);
    let v = dot(dir, q);
    // comiss branches: each rejection is taken when the comparison holds (NaN passes through).
    if !(det > 0.0) || EPSILON > u || u > det || EPSILON > v || v + u > det {
        return false;
    }
    let hit = 1.0 / det * dot(e2, q);
    *t = hit;
    if hit < 0.0 || tmax < hit {
        return false;
    }
    *normal = normalize(cross(e1, e2));
    true
}

impl HeightGrid<'_> {
    /// Terrain ray narrow phase FUN_1400afef0, used to clip EditTool picking to the ground.
    /// Deviation: enumerate the ray's cell rectangle instead of FUN_1400afa00's DDA traversal.
    pub fn intersect(&self, origin: Vec3, direction: Vec3, maximum: f32) -> Option<(f32, Vec3)> {
        let n = self.width as i32 - 1;
        let half = n >> 1;
        let end = [
            origin[0] + direction[0] * maximum,
            origin[1] + direction[1] * maximum,
        ];
        let lo = std::array::from_fn::<_, 2, _>(|k| {
            ((origin[k].min(end[k]) + half as f32).floor() as i32 - 1).clamp(0, n - 1)
        });
        let hi = std::array::from_fn::<_, 2, _>(|k| {
            ((origin[k].max(end[k]) + half as f32).floor() as i32 + 1).clamp(0, n - 1)
        });
        let mut limit = maximum;
        let mut found = false;
        let mut normal = [0.; 3];
        for y in lo[1]..=hi[1] {
            for x in lo[0]..=hi[0] {
                let [a, b, c, d] = [
                    self.corner(x, y, half),
                    self.corner(x + 1, y, half),
                    self.corner(x + 1, y + 1, half),
                    self.corner(x, y + 1, half),
                ];
                let triangles = if (x + y) & 1 == 0 {
                    [[a, b, d], [b, c, d]]
                } else {
                    [[a, b, c], [a, c, d]]
                };
                for [a, b, c] in triangles {
                    let mut t = limit;
                    if ray_triangle(origin, direction, limit, a, b, c, &mut t, &mut normal) {
                        limit = t;
                        found = true;
                    }
                }
            }
        }
        found.then(|| {
            (
                limit,
                [
                    origin[0] + direction[0] * limit,
                    origin[1] + direction[1] * limit,
                    origin[2] + direction[2] * limit,
                ],
            )
        })
    }

    fn corner(&self, x: i32, y: i32, half: i32) -> Vec3 {
        let hi = self.width as i32 - 1;
        let h = self.heights[y.clamp(0, hi) as usize * self.width + x.clamp(0, hi) as usize];
        [
            (x - half) as f32,
            (y - half) as f32,
            h as i32 as f32 * 0.00390625,
        ]
    }

    /// `FUN_1400afef0`: cast a ray against the two triangles of cell `(x, y)`. Returns whether
    /// either triangle was hit and the last `t` written; `normal` takes the last hit's normal.
    fn raycast_cell(
        &self,
        x: i32,
        y: i32,
        dir: Vec3,
        origin: Vec3,
        tmax: f32,
        normal: &mut Vec3,
    ) -> (bool, f32) {
        let half = (self.width as i32 - 1) >> 1;
        let a = self.corner(x, y, half);
        let b = self.corner(x + 1, y, half);
        let c = self.corner(x + 1, y + 1, half);
        let d = self.corner(x, y + 1, half);
        let (first, second) = if (x + y) & 1 == 0 {
            ([a, b, d], [b, c, d])
        } else {
            ([a, b, c], [a, c, d])
        };
        let mut limit = tmax;
        let mut t = 0.0f32;
        let mut hit = false;
        if ray_triangle(
            origin, dir, limit, first[0], first[1], first[2], &mut t, normal,
        ) {
            limit = t;
            hit = true;
        }
        if ray_triangle(
            origin, dir, limit, second[0], second[1], second[2], &mut t, normal,
        ) {
            hit = true;
        }
        (hit, t)
    }

    /// `FUN_1400afef0` straight down from `(x, y, top)`, at most `length` (in the cell under
    /// `x, y`): the hit triangle's normal, zero when nothing is hit.
    pub fn normal_below(&self, x: f32, y: f32, top: f32, length: f32) -> [f32; 3] {
        let half = ((self.width as i32 - 1) >> 1) as f32;
        let mut normal = [0.0; 3];
        self.raycast_cell(
            (x + half) as i32,
            (y + half) as i32,
            [0.0, 0.0, -1.0],
            [x, y, top],
            length,
            &mut normal,
        );
        normal
    }

    /// `FUN_1400b6460`: ground height at a map position (0 when no triangle is hit). `normal`
    /// receives the hit triangle's normal and is left unchanged otherwise.
    pub fn ground(&self, x: f32, y: f32, normal: &mut [f32; 3]) -> f32 {
        let half = ((self.width as i32 - 1) >> 1) as f32;
        let ix = (x + half) as i32;
        let iy = (y + half) as i32;
        let dir = [0.0, 0.0, -1.0];
        let origin = [x, y, RAY_TOP];
        let (hit, t) = self.raycast_cell(ix, iy, dir, origin, RAY_LENGTH, normal);
        if hit {
            t * dir[2] + origin[2]
        } else {
            0.0
        }
    }

    /// `FUN_1400a3790`: the natural type of gameplay map cell `index` (row-major over
    /// `map_size²`). The gameplay map is `n × n` cells centred on the `2n`-cell terrain.
    pub fn natural_cell_type(&self, map_size: usize, index: usize) -> CellType {
        let size = map_size as i32;
        let index = index as i32;
        let offset = (size >> 1) as f32 - 0.5;
        let x = (index % size) as f32 - offset;
        let y = (index / size) as f32 - offset;
        let mut normal = [0.0; 3];
        let h = self.ground(x, y, &mut normal);
        CellType::classify(h, normal[2])
    }
}

/// Map cell types (byte `+0x29` of the 48-byte map cell). Types 5–8 block movement
/// (mask `0x1e0`); the rest are passable (mask `0xe1f`). Names for unconfirmed values are left
/// numeric.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellType(pub u8);

impl CellType {
    pub const GROUND: CellType = CellType(2);
    pub const HIGH: CellType = CellType(6);
    pub const SHALLOW_WATER: CellType = CellType(7);
    pub const DEEP_WATER: CellType = CellType(8);
    pub const ROUGH: CellType = CellType(10);

    /// Thresholds from `FUN_1400a3790`.
    pub fn classify(height: f32, up: f32) -> CellType {
        if height < -1.4 {
            return CellType::DEEP_WATER;
        }
        if height < -0.25 {
            return CellType::SHALLOW_WATER;
        }
        if (-0.25..=0.75).contains(&height) && 0.9 <= up {
            return CellType::GROUND;
        }
        if height < 2.0 {
            CellType::ROUGH
        } else {
            CellType::HIGH
        }
    }

    pub fn blocks_movement(self) -> bool {
        (1u32 << self.0) & 0x1e0 != 0
    }
}
