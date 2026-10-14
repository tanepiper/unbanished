//! Terrain surface mesh (`FUN_1400b16b0`, vertex builder `FUN_1400b1d50`, normal `FUN_1400b3740`).
//!
//! The map is split into 32×32-cell tiles, each with its own 33×33 vertices; all tiles share one
//! index list. Vertices are 16 bytes and match the terrain vertex shader:
//! `i16 x, y` (cell coordinates centred on the map), `i16 height` (the saved `grid × 256` value),
//! `i16 0xff`, `i16 u, v` (map position × 16384, sampling the surface image), and `u8 normal xyz`
//! (`(n + 1) × 127.5`) plus a zero byte.

/// Cells per tile side (MapComponent `+0xc8`).
pub const TILE_CELLS: usize = 32;
/// Vertices per tile side (MapComponent `+0xcc`).
pub const TILE_VERTS: usize = TILE_CELLS + 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(C)]
pub struct TerrainVertex {
    pub pos: [i16; 4],
    pub uv: [i16; 2],
    pub normal: [u8; 4],
}

pub struct TerrainMesh {
    /// Cells per map side (`2n`, MapComponent `+0xc0`).
    pub size: usize,
    pub tiles_per_side: usize,
    /// `tiles_per_side²` blocks of `TILE_VERTS²` vertices, row-major by tile.
    pub vertices: Vec<TerrainVertex>,
    /// Indices for one tile (two triangles per cell).
    pub indices: Vec<u16>,
}

/// Height samples of the saved map (`(size + 1)²` signed values, see `Generator::heights`).
pub struct HeightGrid<'a> {
    pub width: usize,
    pub heights: &'a [i16],
}

impl HeightGrid<'_> {
    fn at(&self, x: i32, y: i32) -> i32 {
        let hi = self.width as i32 - 1;
        self.heights[(y.clamp(0, hi) as usize) * self.width + x.clamp(0, hi) as usize] as i32
    }

    /// `FUN_1400b3740`: normal from integer central differences.
    pub fn normal(&self, x: i32, y: i32) -> [f32; 3] {
        let scale = -1.0 / 256.0;
        let v = [
            (self.at(x + 1, y) - self.at(x - 1, y)) as f32 * scale,
            (self.at(x, y + 1) - self.at(x, y - 1)) as f32 * scale,
            2.0,
        ];
        let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        let inv = 1.0 / len;
        [v[0] * inv, v[1] * inv, v[2] * inv]
    }

    /// `FUN_1400b1d50` for one vertex.
    pub fn vertex(&self, x: i32, y: i32, size: i32) -> TerrainVertex {
        let half = (size >> 1) as i16;
        let n = self.normal(x, y);
        let byte = |v: f32| ((v + 1.0) * 127.5) as i64 as u8;
        TerrainVertex {
            pos: [x as i16 - half, y as i16 - half, self.at(x, y) as i16, 0xff],
            uv: [
                ((x as f32 / size as f32) * 16384.0) as i32 as i16,
                ((y as f32 / size as f32) * 16384.0) as i32 as i16,
            ],
            normal: [byte(n[0]), byte(n[1]), byte(n[2]), 0],
        }
    }
}

impl TerrainMesh {
    pub fn build(grid: &HeightGrid, size: usize) -> TerrainMesh {
        let tiles = size / TILE_CELLS;
        let mut vertices = Vec::with_capacity(tiles * tiles * TILE_VERTS * TILE_VERTS);
        for ty in 0..tiles {
            for tx in 0..tiles {
                for y in 0..TILE_VERTS {
                    for x in 0..TILE_VERTS {
                        vertices.push(grid.vertex(
                            (tx * TILE_CELLS + x) as i32,
                            (ty * TILE_CELLS + y) as i32,
                            size as i32,
                        ));
                    }
                }
            }
        }
        TerrainMesh {
            size,
            tiles_per_side: tiles,
            vertices,
            indices: tile_indices(),
        }
    }

    pub fn vertex_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.vertices.len() * 16);
        for v in &self.vertices {
            for p in v.pos.iter().chain(&v.uv) {
                out.extend_from_slice(&p.to_le_bytes());
            }
            out.extend_from_slice(&v.normal);
        }
        out
    }
}

/// Two triangles per cell; the diagonal alternates with `(x + y) & 1`.
pub fn tile_indices() -> Vec<u16> {
    let w = TILE_VERTS as u16;
    let mut out = Vec::with_capacity(TILE_CELLS * TILE_CELLS * 6);
    for y in 0..TILE_CELLS as u16 {
        for x in 0..TILE_CELLS as u16 {
            let a = y * w + x;
            let b = a + 1;
            let c = (y + 1) * w + x;
            if (x + y) & 1 == 0 {
                out.extend_from_slice(&[a, c, b, b, c, c + 1]);
            } else {
                out.extend_from_slice(&[a, c + 1, b, a, c, c + 1]);
            }
        }
    }
    out
}
