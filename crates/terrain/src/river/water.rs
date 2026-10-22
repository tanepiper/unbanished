//! The water mesh of a river: its vertices and its strip.

use super::*;

// MapComponent +0x1a0 (vertices), +0x1b0 (indices), +0x1c0 / +0x1d0 (the ranges).
/// The map's water mesh: the strips of the rivers, then the surfaces of the lakes.
///
/// A vertex is 16 bytes: `i16 x, y` (1/48 cell), `i16 z` (1/256), `0`, `i16 u, v`, the normal
/// bytes `0x7f, 0x7f`, an alpha byte, and `0`.
///
/// Original: `FUN_1400b0730`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WaterMesh {
    /// The vertices.
    pub vertices: Vec<[u8; 16]>,
    /// The indices, each relative to the first vertex of its range.
    pub indices: Vec<u16>,
    /// The range of each river: first index, first vertex, vertex count, index count.
    pub rivers: Vec<[i32; 4]>,
    /// The range of each lake, in the same form as [`rivers`](Self::rivers).
    pub lakes: Vec<[i32; 4]>,
}

pub(super) fn water_vertex(x: i16, y: i16, z: i16, u: i16, v: i16, alpha: u8) -> [u8; 16] {
    let mut out = [0u8; 16];
    for (k, s) in [x, y, z, 0, u, v].into_iter().enumerate() {
        out[2 * k..2 * k + 2].copy_from_slice(&s.to_le_bytes());
    }
    out[12] = 0x7f;
    out[13] = 0x7f;
    out[14] = alpha;
    out
}

// Reads MapComponent +0xc0, +0xd0, +0xd8 and the generator's +0x18 (paths), +0x48 (lakes).
/// The water mesh of a generated map. `paths` and `lakes` are the generator's, `heights` is the
/// map's `(size + 1)²` height grid, and `size` is its size in cells.
///
/// Original: `FUN_1400b0730`.
pub fn water_mesh(
    paths: &[Vec<PathPoint>],
    lakes: &[[f32; 3]],
    heights: &[i16],
    size: usize,
) -> WaterMesh {
    let mut mesh = WaterMesh::default();
    // Each river a little lower than the one before (`−192 − i`, 1/256).
    for (i, path) in paths.iter().enumerate() {
        let (vertices, indices) = river_strip(path);
        mesh.rivers.push([
            mesh.indices.len() as i32,
            mesh.vertices.len() as i32,
            vertices.len() as i32,
            indices.len() as i32,
        ]);
        let z = (-192.0 - i as f32) as i32 as i16;
        mesh.vertices.extend(vertices.iter().map(|v| {
            water_vertex(
                (v[0] * 48.0) as i32 as i16,
                (v[1] * 48.0) as i32 as i16,
                z,
                (v[2] * 1024.0) as i32 as i16,
                (v[3] * 1024.0) as i32 as i16,
                0xff,
            )
        }));
        mesh.indices.extend_from_slice(&indices);
    }
    let width = size + 1;
    let half = (size >> 1) as i32;
    let height = |gx: i32, gy: i32| -> f32 {
        let (x, y) = (gx.clamp(0, width as i32 - 1), gy.clamp(0, width as i32 - 1));
        heights[y as usize * width + x as usize] as i32 as f32 * 0.00390625
    };
    for (i, lake) in lakes.iter().enumerate() {
        let [cx, cy, r] = *lake;
        let outer = r + 2.0;
        let first_index = mesh.indices.len();
        let first_vertex = mesh.vertices.len();
        let (x0, y0) = ((cx - outer) as i32, (cy - outer) as i32);
        let (x1, y1) = ((cx + outer) as i32, (cy + outer) as i32);
        let fade = outer - r;
        let z = (i as f32 - 191.0) as i32 as i16;
        let mut local: Vec<[u8; 16]> = Vec::new();
        let mut seen: HashMap<[u8; 16], u16> = HashMap::new();
        let mut indices = Vec::new();
        for y in y0..y1 {
            for x in x0..x1 {
                let corners = [
                    (x, y, height(half + x, half + y)),
                    (x + 1, y, height(half + 1 + x, half + y)),
                    (x + 1, y + 1, height(half + 1 + x, half + 1 + y)),
                    (x, y + 1, height(half + x, half + 1 + y)),
                ];
                let mut above = true;
                let mut clear = true;
                let mut alpha = [0.0f32; 4];
                for (k, &(px, py, pz)) in corners.iter().enumerate() {
                    if pz <= -0.75 {
                        above = false;
                    }
                    let d = [px as f32 - cx, py as f32 - cy, pz - 0.0];
                    // `Vector3::Dot`: `(y² + x²) + z²`.
                    let dist = (d[1] * d[1] + d[0] * d[0] + d[2] * d[2]).sqrt();
                    let a = 1.0 - (dist - r) / fade;
                    alpha[k] = if a < 0.0 {
                        0.0
                    } else if 1.0 < a {
                        clear = false;
                        1.0
                    } else {
                        if 0.0 < a {
                            clear = false;
                        }
                        a
                    };
                }
                if clear || above {
                    continue;
                }
                let mut quad = [0u16; 4];
                for (k, &(px, py, _)) in corners.iter().enumerate() {
                    let (vx, vy) = (
                        (px as f32 * 48.0) as i32 as i16,
                        (py as f32 * 48.0) as i32 as i16,
                    );
                    let v = water_vertex(vx, vy, z, vx, vy, (alpha[k] * 255.0) as i64 as u8);
                    quad[k] = *seen.entry(v).or_insert_with(|| {
                        local.push(v);
                        (local.len() - 1) as u16
                    });
                }
                indices.extend_from_slice(&[quad[0], quad[1], quad[2], quad[0], quad[2], quad[3]]);
            }
        }
        mesh.lakes.push([
            first_index as i32,
            first_vertex as i32,
            local.len() as i32,
            indices.len() as i32,
        ]);
        mesh.vertices.extend(local);
        mesh.indices.extend(indices);
    }
    mesh
}
