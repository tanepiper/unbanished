//! The terrain mesh that the renderer draws.
//!
//! The map is split into 32×32-cell tiles, each with its own 33×33 vertices; all tiles share one
//! index list. Vertices are 16 bytes and match the terrain vertex shader: `i16 x, y` (cell
//! coordinates centred on the map), `i16 height` (the saved `grid × 256` value), `i16 0xff`, `i16
//! u, v` (map position × 16384, sampling the surface image), and `u8 normal xyz` (`(n + 1) ×
//! 127.5`) plus a zero byte.
//!
//! Original: `FUN_1400b16b0` (the mesh), `FUN_1400b1d50` (a vertex), `FUN_1400b3740` (a normal).

// MapComponent +0xc8
/// The number of cells on each side of a tile.
pub const TILE_CELLS: usize = 32;
// MapComponent +0xcc
/// The number of vertices on each side of a tile.
pub const TILE_VERTS: usize = TILE_CELLS + 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(C)]
/// One vertex of the terrain mesh, 16 bytes, as the terrain shader reads it.
pub struct TerrainVertex {
    /// `x`, `y` (cells from the centre of the terrain), the height (the saved value, height ×
    /// 256), and `0xff`.
    pub pos: [i16; 4],
    /// The position on the map × 16384, to sample the surface image.
    pub uv: [i16; 2],
    /// The normal as bytes, `(n + 1) × 127.5`, and a zero byte.
    pub normal: [u8; 4],
}

/// The terrain mesh: tiles of [`TILE_CELLS`] × [`TILE_CELLS`] cells, each with its own vertices,
/// and one index list that every tile uses.
pub struct TerrainMesh {
    // MapComponent +0xc0
    /// The terrain's size in cells, `2n`.
    pub size: usize,
    /// The number of tiles on each side.
    pub tiles_per_side: usize,
    /// The vertices: `tiles_per_side²` blocks of `TILE_VERTS²` vertices, tile by tile, row by row.
    pub vertices: Vec<TerrainVertex>,
    /// Indices for one tile (two triangles per cell).
    pub indices: Vec<u16>,
}

/// The heights of a map as a save stores them, for queries and for the mesh: `width²` values, row
/// by row (see [`Generator::heights`](crate::Generator::heights)).
pub struct HeightGrid<'a> {
    /// The number of samples on each side, `2n + 1`.
    pub width: usize,
    /// The heights, height × 256.
    pub heights: &'a [i16],
}

impl HeightGrid<'_> {
    fn at(&self, x: i32, y: i32) -> i32 {
        let hi = self.width as i32 - 1;
        self.heights[(y.clamp(0, hi) as usize) * self.width + x.clamp(0, hi) as usize] as i32
    }

    /// Normal from integer central differences.
    ///
    /// Original: `FUN_1400b3740`.
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

    /// The vertex at grid point `(x, y)` of a terrain of `size` cells.
    ///
    /// Original: `FUN_1400b1d50`.
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
    /// The mesh of a terrain of `size` cells.
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

    /// The vertices as bytes, ready for a GPU vertex buffer.
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
