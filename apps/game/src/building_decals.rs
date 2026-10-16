//! The entities' terrain decals: a DecalComponent (activate `FUN_140120a30`) adds its
//! description's decal over the entity's map rectangle (`FUN_1400b6250`, the map's `+0x1f0`
//! list), and `FUN_1400b2310` meshes it: a quad per cell, its corners on the terrain like the
//! roads' (`FUN_1400b20e0`). See docs/DECALS.md.
//!
//! - a picture (building footprints): the atlas uv is the full tile (`DAT_1401ca450[255]`),
//!   and the decal uv maps the rectangle onto the variant's texture rectangle, turned with the
//!   entity: `(x, 1 − y)`, about the centre by the map rotation, into `rect / scale`, × 4096;
//! - tiled (fields, yards, pastures): the atlas tile follows which neighbours are inside the
//!   rectangle, and the decal uv is the cell position, turned.
//!
//! The alpha is the description's until the entity is built (`banished_game::construction::
//! Decal`), the material the component's.

use std::collections::BTreeMap;
use std::sync::Arc;

use banished_formats::game::DecalDescription;
use banished_formats::pkg::Archive;
use banished_game::construction::Decal;
use banished_game::seeding::{Placed, Template};
use banished_game::terrain::HeightGrid;

use crate::road_decals::{tile_uv, tile_variant, vertex};
use crate::scene_assets::{DecalCache, DecalData};

/// Each template's DecalDescription (`Template\X.rsc:decal`), if it has one.
pub fn descriptions(
    archive: &Archive,
    templates: &[Template],
) -> Vec<Option<Arc<DecalDescription>>> {
    templates
        .iter()
        .map(|t| {
            let reference = t.reference.replace('\\', "/");
            let reference = reference.strip_suffix(":resource")?;
            let data = archive.read_ref(&format!("{reference}:decal")).ok()?;
            banished_formats::game::decal_description(&data)
                .ok()
                .map(Arc::new)
        })
        .collect()
}

/// One decal to mesh: the map rectangle (cells, inclusive), the rotation (degrees), the
/// texture rectangle, the alpha byte, tiled.
struct Entry<'a> {
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    yaw: f32,
    rect: [f32; 4],
    scale: f32,
    alpha: u8,
    tiled: bool,
    material: &'a str,
}

/// A 2D affine map in the engine's row-vector order (`Math::Transform2`): `p' = p·M + t`.
#[derive(Clone, Copy)]
struct Transform2 {
    m: [f32; 4],
    t: [f32; 2],
}

impl Transform2 {
    fn new(sx: f32, sy: f32, tx: f32, ty: f32) -> Self {
        Transform2 {
            m: [sx, 0.0, 0.0, sy],
            t: [tx, ty],
        }
    }

    /// `Transform2(Scalar, Vector2)` / `FromYawOffset`: rows `(cos, sin)`, `(−sin, cos)`.
    fn rotation(degrees: f32) -> Self {
        let (s, c) = (degrees * 0.017453292).sin_cos();
        Transform2 {
            m: [c, s, -s, c],
            t: [0.0, 0.0],
        }
    }

    /// `operator*`: `self` first, then `o`.
    fn then(self, o: Transform2) -> Self {
        let [a0, a1, a2, a3] = self.m;
        let [b0, b1, b2, b3] = o.m;
        Transform2 {
            m: [
                a0 * b0 + a1 * b2,
                a0 * b1 + a1 * b3,
                a2 * b0 + a3 * b2,
                a2 * b1 + a3 * b3,
            ],
            t: [
                self.t[0] * b0 + self.t[1] * b2 + o.t[0],
                self.t[0] * b1 + self.t[1] * b3 + o.t[1],
            ],
        }
    }

    fn apply(&self, p: [f32; 2]) -> [f32; 2] {
        [
            p[0] * self.m[0] + p[1] * self.m[2] + self.t[0],
            p[0] * self.m[1] + p[1] * self.m[3] + self.t[1],
        ]
    }
}

/// `FUN_1400b2310`'s uv chain for the rectangle's unit square.
fn uv_transform(e: &Entry) -> Transform2 {
    let (w, h) = ((e.x1 - e.x0 + 1) as f32, (e.y1 - e.y0 + 1) as f32);
    let (place, rect, scale, half) = if e.tiled {
        (
            Transform2::new(w, h, e.x0 as f32, e.y0 as f32),
            Transform2::new(1.0, 1.0, 0.0, 0.0),
            1.0,
            0.0,
        )
    } else {
        let [u0, v0, u1, v1] = e.rect;
        let s = e.scale;
        (
            Transform2::new(1.0, -1.0, 0.0, 1.0),
            Transform2::new((u1 - u0) / s, (v1 - v0) / s, u0 / s, v0 / s),
            4096.0,
            0.5,
        )
    };
    place
        .then(Transform2::new(1.0, 1.0, -half, -half))
        .then(Transform2::rotation(e.yaw))
        .then(Transform2::new(1.0, 1.0, half, half))
        .then(rect)
        .then(Transform2::new(scale, scale, 0.0, 0.0))
}

/// `FUN_1400b2310`: the decal's cells, `(x, y)` its grid offset.
fn mesh(e: &Entry, grid: &HeightGrid, size: i32, off: i32) -> (Vec<u8>, Vec<u16>) {
    let f = uv_transform(e);
    let (w, h) = ((e.x1 - e.x0 + 1) as f32, (e.y1 - e.y0 + 1) as f32);
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for y in e.y0..=e.y1 {
        for x in e.x0..=e.x1 {
            let variant = if e.tiled {
                let inside = |dx: i32, dy: i32| {
                    (e.x0..=e.x1).contains(&(x + dx)) && (e.y0..=e.y1).contains(&(y + dy))
                };
                tile_variant([
                    inside(-1, 0),
                    inside(1, 0),
                    inside(0, -1),
                    inside(0, 1),
                    inside(-1, -1),
                    inside(1, -1),
                    inside(-1, 1),
                    inside(1, 1),
                ])
            } else {
                46
            };
            let [u0, v0, u1, v1] = tile_uv(variant);
            let base = (vertices.len() / 20) as u16;
            for (dx, dy, tu, tv) in [
                (0, 0, u0, v0),
                (1, 0, u1, v0),
                (1, 1, u1, v1),
                (0, 1, u0, v1),
            ] {
                let mut v = vertex(grid, size, x + dx + off, y + dy + off, e.alpha);
                let p = [(x + dx - e.x0) as f32 / w, (y + dy - e.y0) as f32 / h];
                let [du, dv] = f.apply(p);
                v[8..10].copy_from_slice(&(du as i32 as i16).to_le_bytes());
                v[10..12].copy_from_slice(&(dv as i32 as i16).to_le_bytes());
                v[12..14].copy_from_slice(&tu.to_le_bytes());
                v[14..16].copy_from_slice(&tv.to_le_bytes());
                vertices.extend_from_slice(&v);
            }
            let quad: [u16; 6] = if (x + y) & 1 == 0 {
                [0, 3, 1, 1, 3, 2]
            } else {
                [0, 2, 1, 0, 3, 2]
            };
            indices.extend(quad.iter().map(|i| base + i));
        }
    }
    (vertices, indices)
}

/// Every live entity's decal, in entity order.
pub fn build(
    archive: &Archive,
    placed: &[Placed],
    descriptions: &[Option<Arc<DecalDescription>>],
    decals: &BTreeMap<usize, Decal>,
    grid: &HeightGrid,
    cells: usize,
    cache: &mut DecalCache,
) -> Vec<DecalData> {
    let size = grid.width as i32 - 1;
    let off = (size - cells as i32) >> 1;
    let mut out = Vec::new();
    for (i, p) in placed.iter().enumerate() {
        let Some(d) = descriptions.get(p.template).and_then(Option::as_deref) else {
            continue;
        };
        if !p.alive || p.footprint.0 <= 0 || p.footprint.1 <= 0 {
            continue;
        }
        let state = decals.get(&i).copied().unwrap_or_default();
        let Some(Some(material)) = usize::try_from(state.material)
            .ok()
            .and_then(|m| d.materials.get(m))
        else {
            continue;
        };
        let alpha = if state.opaque { 1.0 } else { d.alpha };
        let e = Entry {
            x0: p.cell.0,
            y0: p.cell.1,
            x1: p.cell.0 + p.footprint.0 - 1,
            y1: p.cell.1 + p.footprint.1 - 1,
            yaw: p.yaw,
            rect: d
                .rects
                .get(p.variant)
                .copied()
                .unwrap_or([0.0, 0.0, 1.0, 1.0]),
            scale: d.scale,
            alpha: (alpha * 255.0) as i64 as u8,
            tiled: d.tiled,
            material,
        };
        let (vertices, indices) = mesh(&e, grid, size, off);
        if let Some(d) = DecalData::from_mesh(archive, e.material, vertices, &indices, cache) {
            out.push(d);
        }
    }
    out
}

/// What the decals are built from, to rebuild them when it changes.
pub fn signature(
    placed: &[Placed],
    descriptions: &[Option<Arc<DecalDescription>>],
    decals: &BTreeMap<usize, Decal>,
) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for (i, p) in placed.iter().enumerate() {
        if !p.alive || descriptions.get(p.template).is_none_or(Option::is_none) {
            continue;
        }
        (i, p.cell, p.footprint, p.yaw.to_bits(), p.variant).hash(&mut h);
        if let Some(d) = decals.get(&i) {
            (d.opaque, d.material).hash(&mut h);
        }
    }
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(yaw: f32) -> Entry<'static> {
        Entry {
            x0: 10,
            y0: 20,
            x1: 12,
            y1: 23,
            yaw,
            rect: [256.0, 0.0, 512.0, 320.0],
            scale: 2048.0,
            alpha: 127,
            tiled: false,
            material: "",
        }
    }

    #[test]
    fn a_picture_maps_onto_its_texture_rectangle() {
        let f = uv_transform(&entry(0.0));
        // The rectangle's top-left corner takes the texture's bottom-left (v flipped).
        let close =
            |a: [f32; 2], b: [f32; 2]| (a[0] - b[0]).abs() < 0.01 && (a[1] - b[1]).abs() < 0.01;
        assert!(close(f.apply([0.0, 0.0]), [512.0, 640.0]));
        assert!(close(f.apply([1.0, 1.0]), [1024.0, 0.0]));
        // Turned a half turn, the corners swap.
        let g = uv_transform(&entry(180.0));
        assert!(close(g.apply([0.0, 0.0]), [1024.0, 0.0]));
    }

    #[test]
    fn a_tiled_decal_uses_the_cell_positions() {
        let mut e = entry(0.0);
        e.tiled = true;
        let f = uv_transform(&e);
        assert_eq!(f.apply([0.0, 0.0]), [10.0, 20.0]);
        assert_eq!(f.apply([1.0, 1.0]), [13.0, 24.0]);
    }
}

#[cfg(test)]
mod scene_check {
    use super::*;
    use banished_formats::pkg::{Archive, Package};

    /// The menu scenes' serialized map decals (`+0x1f0`) were meshed by the original:
    /// rebuilding each from its own entry gives the same uvs.
    #[test]
    fn menu_scene_decals_match() {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../game-data/original/WinData");
        let mut archive = Archive::default();
        for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
            let Ok(bytes) = std::fs::read(dir.join(name)) else {
                return;
            };
            archive.add(Package::parse(bytes).unwrap());
        }
        let mut checked = (0, 0);
        for i in 0..10 {
            let data = archive
                .read_ref(&format!("Game/MenuScene/Scene{i}.rsc:resource"))
                .unwrap();
            let scene = banished_formats::menu_scene::menu_scene(&data).unwrap();
            let heights = &scene.map.heights;
            let width = (heights.len() as f64).sqrt() as usize;
            let grid = HeightGrid { width, heights };
            for d in &scene.map.extra_decals {
                let [_, x0, y0, x1, y1] = d.ints;
                let e = Entry {
                    x0,
                    y0,
                    x1,
                    y1,
                    yaw: d.int6 as f32,
                    rect: [d.floats[1], d.floats[2], d.floats[3], d.floats[4]],
                    scale: d.floats[0],
                    alpha: d.byte,
                    tiled: d.flag7,
                    material: "",
                };
                let (vertices, _) = mesh(&e, &grid, width as i32 - 1, 0);
                // The decal uv may differ by one where the original's float rounding falls
                // the other way (turned rectangles); the atlas uv is exact.
                let uv = |v: &[u8]| -> [i16; 4] {
                    std::array::from_fn(|k| i16::from_le_bytes([v[8 + 2 * k], v[9 + 2 * k]]))
                };
                let mut theirs: Vec<[i16; 4]> = d.vertices.iter().map(|v| uv(v)).collect();
                let all = vertices.chunks(20).all(|v| {
                    let o = uv(v);
                    let near = |t: &[i16; 4]| {
                        (t[0] - o[0]).abs() <= 1 && (t[1] - o[1]).abs() <= 1 && t[2..] == o[2..]
                    };
                    theirs
                        .iter()
                        .position(near)
                        .map(|i| theirs.swap_remove(i))
                        .is_some()
                        || d.vertices.iter().any(|t| near(&uv(t)))
                });
                if all && vertices.len() / 20 >= d.vertices.len() {
                    checked.0 += 1;
                } else {
                    checked.1 += 1;
                    eprintln!(
                        "scene {i} {:?} {:?} tiled {} yaw {}",
                        d.material, d.ints, d.flag7, d.int6
                    );
                }
            }
        }
        assert!(1000 < checked.0, "{checked:?}");
        assert_eq!(checked.1, 0);
    }
}
