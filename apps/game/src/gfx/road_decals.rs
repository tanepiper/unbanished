//! The roads' tiled decals (`FUN_1400b4840`): per 32-cell tile and road kind, a quad on every
//! cell with that kind, its corners on the terrain (`FUN_1400b1f70`), its alpha the cell's
//! built byte (a road being built shows faintly), and its tile in the road atlas chosen by its
//! neighbours (`FUN_1400b3650`). See docs/ROADS.md.
//!
//! The kinds' materials are the road templates' DecalDescriptions (`Template\StoneRoad.rsc`,
//! kind 0, and `Template\DirtRoad.rsc`, kind 1), the map's `+0x210` list.

use unbanished_formats::pkg::Archive;
use unbanished_game::nature::seeding::Cells;
use unbanished_game::terrain::HeightGrid;

use crate::view::scene_assets::DecalData;

/// `+0x200`'s tile: cells per side.
const TILE: i32 = 32;

/// `FUN_1400b3650`: the atlas tile of a cell from which neighbours share its decal (left,
/// right, up, down, then the corners up-left, up-right, down-left, down-right).
pub(crate) fn tile_variant(n: [bool; 8]) -> u8 {
    let [l, r, u, d, ul, ur, dl, dr] = n;
    let corner = |c: bool, a: bool, b: bool| c && a && b;
    let mask = l as usize
        | (r as usize) << 1
        | (u as usize) << 2
        | (d as usize) << 3
        | (corner(ul, l, u) as usize) << 4
        | (corner(ur, r, u) as usize) << 5
        | (corner(dl, l, d) as usize) << 6
        | (corner(dr, r, d) as usize) << 7;
    VARIANTS[mask]
}

/// `DAT_1401ca450`: the atlas tile by the neighbour mask (left 1, right 2, up 4, down 8, then
/// the corners up-left 0x10, up-right 0x20, down-left 0x40, down-right 0x80, each only with both
/// its edges); 255 where no mask leads.
const VARIANTS: [u8; 256] = [
    0, 1, 2, 5, 3, 6, 7, 11, 4, 8, 9, 12, 10, 13, 14, 19, 255, 255, 255, 255, 255, 15, 255, 20,
    255, 255, 255, 255, 255, 21, 255, 28, 255, 255, 255, 255, 255, 255, 16, 22, 255, 255, 255, 255,
    255, 255, 23, 29, 255, 255, 255, 255, 255, 255, 255, 30, 255, 255, 255, 255, 255, 255, 255, 36,
    255, 255, 255, 255, 255, 255, 255, 255, 255, 17, 255, 24, 255, 25, 255, 31, 255, 255, 255, 255,
    255, 255, 255, 255, 255, 255, 255, 255, 255, 32, 255, 37, 255, 255, 255, 255, 255, 255, 255,
    255, 255, 255, 255, 255, 255, 255, 255, 38, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    255, 255, 255, 255, 255, 42, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 18, 26, 255,
    255, 27, 33, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 39,
    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 34, 40, 255, 255, 255,
    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 43, 255, 255, 255, 255, 255, 255,
    255, 255, 255, 255, 255, 35, 255, 255, 255, 41, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    255, 255, 255, 255, 255, 255, 44, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    255, 255, 255, 45, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    46,
];

/// `DAT_1401ca2d0`: each atlas tile's `u0, v0, u1, v1` (an 8×6 grid of 512-unit cells, inset
/// by 128).
pub(crate) fn tile_uv(variant: u8) -> [i16; 4] {
    if variant == 46 {
        return [3200, 2688, 3200, 2688];
    }
    if 46 < variant {
        return [0; 4];
    }
    let (col, row) = (variant as i16 % 8, variant as i16 / 8);
    [
        128 + 512 * col,
        128 + 512 * row,
        384 + 512 * col,
        384 + 512 * row,
    ]
}

/// The road templates' decal materials, by kind.
pub fn materials(archive: &Archive) -> Vec<Option<String>> {
    [
        "Template/StoneRoad.rsc:decal",
        "Template/DirtRoad.rsc:decal",
    ]
    .iter()
    .map(|r| {
        let data = archive.read_ref(r).ok()?;
        unbanished_formats::game::decal_description(&data)
            .ok()?
            .materials
            .into_iter()
            .next()
            .flatten()
    })
    .collect()
}

/// `FUN_1400b1f70`: a corner's 20-byte vertex (position, height, the built byte as alpha, the
/// terrain's uv, the atlas uv filled in by the caller, the normal). The height grid is wider
/// than the cells (`+0xc0` against `+0xc4`); `(gx, gy)` is the corner in it.
pub(crate) fn vertex(grid: &HeightGrid, size: i32, gx: i32, gy: i32, built: u8) -> [u8; 20] {
    let half = size >> 1;
    let hi = grid.width as i32 - 1;
    let z = grid.heights[gy.clamp(0, hi) as usize * grid.width + gx.clamp(0, hi) as usize];
    let n = grid.normal(gx, gy);
    let byte = |v: f32| ((v + 1.0) * 127.5) as i64 as u8;
    let mut v = [0u8; 20];
    let put = |v: &mut [u8; 20], at: usize, s: i16| v[at..at + 2].copy_from_slice(&s.to_le_bytes());
    put(&mut v, 0, (gx - half) as i16);
    put(&mut v, 2, (gy - half) as i16);
    put(&mut v, 4, z);
    put(&mut v, 6, built as i16);
    put(
        &mut v,
        8,
        ((gx as f32 / size as f32) * 16384.0) as i32 as i16,
    );
    put(
        &mut v,
        10,
        ((gy as f32 / size as f32) * 16384.0) as i32 as i16,
    );
    v[16] = byte(n[0]);
    v[17] = byte(n[1]);
    v[18] = byte(n[2]);
    v
}

/// `FUN_1400b4840`: the decals of every tile and kind.
pub fn build(
    archive: &Archive,
    cells: &Cells,
    grid: &HeightGrid,
    materials: &[Option<String>],
    cache: &mut crate::view::scene_assets::DecalCache,
) -> Vec<DecalData> {
    let n = cells.size as i32;
    // `+0xc0`: the height grid's cells; the cells sit in its middle.
    let size = grid.width as i32 - 1;
    let off = (size - n) >> 1;
    let kind_at = |x: i32, y: i32| -> u8 {
        if x < 0 || y < 0 || n <= x || n <= y {
            return 0xff;
        }
        cells.road_kind[(y * n + x) as usize]
    };
    let mut out = Vec::new();
    for ty in (0..n).step_by(TILE as usize) {
        for tx in (0..n).step_by(TILE as usize) {
            let mut meshes: Vec<(Vec<u8>, Vec<u16>)> = vec![Default::default(); materials.len()];
            for y in ty..(ty + TILE).min(n) {
                for x in tx..(tx + TILE).min(n) {
                    let kind = kind_at(x, y);
                    let Some((vertices, indices)) = meshes.get_mut(kind as usize) else {
                        continue;
                    };
                    let built = cells.road_built[(y * n + x) as usize];
                    // Any road kind counts as a neighbour.
                    let at = |dx: i32, dy: i32| kind_at(x + dx, y + dy) != 0xff;
                    let [u0, v0, u1, v1] = tile_uv(tile_variant([
                        at(-1, 0),
                        at(1, 0),
                        at(0, -1),
                        at(0, 1),
                        at(-1, -1),
                        at(1, -1),
                        at(-1, 1),
                        at(1, 1),
                    ]));
                    let base = (vertices.len() / 20) as u16;
                    for (cx, cy, u, v) in [
                        (x, y, u0, v0),
                        (x + 1, y, u1, v0),
                        (x + 1, y + 1, u1, v1),
                        (x, y + 1, u0, v1),
                    ] {
                        let mut vert = vertex(grid, size, cx + off, cy + off, built);
                        vert[12..14].copy_from_slice(&u.to_le_bytes());
                        vert[14..16].copy_from_slice(&v.to_le_bytes());
                        vertices.extend_from_slice(&vert);
                    }
                    // The diagonal alternates with the cell's parity.
                    let quad: [u16; 6] = if (x + y) & 1 == 0 {
                        [0, 3, 1, 1, 3, 2]
                    } else {
                        [0, 2, 1, 0, 3, 2]
                    };
                    indices.extend(quad.iter().map(|i| base + i));
                }
            }
            for (kind, (vertices, indices)) in meshes.into_iter().enumerate() {
                let Some(m) = materials.get(kind).cloned().flatten() else {
                    continue;
                };
                if let Some(d) = DecalData::from_mesh(archive, &m, vertices, &indices, cache) {
                    out.push(d);
                }
            }
        }
    }
    out
}
