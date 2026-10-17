//! Building placement (`PlacementTool`, vtable `0x140168130`): the footprint under the cursor
//! (`FUN_140102ed0`), whether it may be built (`FUN_1400a2a50` → `FUN_1400a4aa0`) and the map
//! stamp of a footprint string (`FUN_1400a30f0`). See docs/BUILD-TOOLS.md.

use crate::seeding::Cells;
use crate::terrain::{CellType, HeightGrid};

/// `DAT_140178904`: degrees per rotation step.
pub const ROTATION_STEP: f32 = 90.0;
/// `DAT_140178948`: height tolerance placement passes (effectively unlimited).
pub const HEIGHT_TOLERANCE: f32 = 1000.0;

/// A cell rectangle, inclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
}

impl Rect {
    pub fn width(&self) -> i32 {
        self.x1 - self.x0 + 1
    }
    pub fn height(&self) -> i32 {
        self.y1 - self.y0 + 1
    }
}

/// `FUN_140102ed0`: the footprint of a `width × height` building centred on `cell` (even sizes
/// extend one more cell towards +x/+y), turned by `rotation` steps (1 and 3 swap the sides).
/// `None` when it leaves the map (the left column must also be at least 1).
pub fn footprint(
    map_size: i32,
    cell: (i32, i32),
    width: i32,
    height: i32,
    rotation: i32,
) -> Option<Rect> {
    let (w, h) = if rotation == 1 || rotation == 3 {
        (height, width)
    } else {
        (width, height)
    };
    let span = |c: i32, s: i32| {
        if s & 1 == 0 {
            (c - (s >> 1) + 1, (s >> 1) + c)
        } else {
            let half = (s - 1) >> 1;
            (c - half, half + c)
        }
    };
    let (x0, x1) = span(cell.0, w);
    let (y0, y1) = span(cell.1, h);
    let n = map_size;
    if x0 < 0 || n <= x0 || y0 < 0 || n <= y0 || x0 < 1 || n <= x1 || y1 < 0 || n <= y1 {
        return None;
    }
    Some(Rect { x0, y0, x1, y1 })
}

/// Index into a footprint string (rows separated by one character, last row first) of cell
/// `(i, j)` of a `w × h` rectangle placed with `degrees` rotation (`FUN_1400a2a50`,
/// `FUN_1400a30f0`; `w`, `h` are the rotated sides).
pub fn footprint_index(i: i32, j: i32, w: i32, h: i32, degrees: i32) -> usize {
    (match degrees {
        90 => i * (h + 1) + j,
        180 => j * (w + 1) + (w - 1 - i),
        270 => (w - 1 - i) * (h + 1) + (h - 1 - j),
        _ => (h - 1 - j) * (w + 1) + i,
    }) as usize
}

/// The `CreatePlacedDescription` and `MapDescription` values placement reads.
#[derive(Debug, Clone)]
pub struct PlacementRules {
    /// `+0x20`: allowed cell types (bit per type) when no per-character table applies.
    pub mask: u32,
    /// `+0x30` / `+0x28`: per-character masks (`'0' + k` selects `masks[k]`) and the string
    /// choosing them per cell.
    pub masks: Vec<u32>,
    pub mask_chars: String,
    /// `MapDescription +0x28`: `#` own type, `_` ground, `.` untouched.
    pub footprint: String,
    /// `+0x44`, `+0x48`.
    pub width: i32,
    pub height: i32,
    /// `MapDescription +0x20`.
    pub cell_type: u8,
}

/// `FUN_1400a4aa0(map, x, y, mask, min_normal, tolerance, reference, override_occupant,
/// override_flag, owner)`. `owner_ok(occupant)` stands for the occupant's template being the
/// owner template (`+0x28 == owner`).
#[allow(clippy::too_many_arguments)]
pub fn cell_admits(
    cells: &Cells,
    terrain: &HeightGrid,
    x: i32,
    y: i32,
    mask: u32,
    min_normal: f32,
    tolerance: f32,
    reference: f32,
    override_occupant: bool,
    override_flag: bool,
    owner_ok: impl Fn(usize) -> bool,
) -> bool {
    let n = cells.size as i32;
    if x < 0 || n <= x || y < 0 || n <= y {
        return false;
    }
    let i = (n * y + x) as usize;
    if !mask & (1u32 << (cells.types[i].0 & 0x1f)) != 0 {
        return false;
    }
    let occupant = cells.occupants[i];
    if !(occupant.is_none_or(&owner_ok) || override_occupant) {
        return false;
    }
    if !(override_flag || cells.flags[i] == 0) {
        return false;
    }
    let half = (n >> 1) as f32 - 0.5;
    let (px, py) = ((i as i32 % n) as f32 - half, (i as i32 / n) as f32 - half);
    let mut normal = [0.0f32; 3];
    let h = cells
        .floors
        .ground_normal(cells.size, terrain, px, py, &mut normal);
    min_normal <= normal[2] && (h - reference).abs() <= tolerance
}

/// `FUN_1400a2a50` as the placement tool calls it: every footprint cell admits the building.
pub fn can_place(
    cells: &Cells,
    terrain: &HeightGrid,
    rules: &PlacementRules,
    rect: Rect,
    degrees: i32,
    owner_ok: impl Fn(usize) -> bool,
) -> bool {
    (rect.y0..=rect.y1).all(|y| {
        (rect.x0..=rect.x1)
            .all(|x| footprint_cell(cells, terrain, rules, rect, degrees, x, y, &owner_ok).0)
    })
}

/// One cell of a footprint, as `FUN_1400a2a50` and the overlay (`FUN_140106850`) test it:
/// whether it admits the building (`FUN_1400a4aa0` with the cell's mask: `+0x30`'s entry for
/// the `+0x28` character, else `+0x20`) and whether its footprint character is `.` (untouched;
/// its zone count `+0x28` is then not checked).
#[allow(clippy::too_many_arguments)]
pub fn footprint_cell(
    cells: &Cells,
    terrain: &HeightGrid,
    rules: &PlacementRules,
    rect: Rect,
    degrees: i32,
    x: i32,
    y: i32,
    owner_ok: impl Fn(usize) -> bool,
) -> (bool, bool) {
    let (w, h) = (rect.width(), rect.height());
    let k = footprint_index(x - rect.x0, y - rect.y0, w, h, degrees);
    let untouched = rules.footprint.as_bytes().get(k) == Some(&b'.');
    let mut mask = rules.mask;
    if !rules.masks.is_empty() {
        if let Some(&c) = rules.mask_chars.as_bytes().get(k) {
            if let Some(&m) = rules.masks.get((c as i32 - 0x30) as usize) {
                mask = m;
            }
        }
    }
    let admits = cell_admits(
        cells,
        terrain,
        x,
        y,
        mask,
        0.0,
        HEIGHT_TOLERANCE,
        0.0,
        false,
        untouched,
        owner_ok,
    );
    (admits, untouched)
}

/// `FUN_1400a30f0`: stamp a footprint. `#` and an empty footprint set the building's type, `_`
/// sets ground (2); every cell except `.` records the entity at `+0x08`.
pub fn stamp(cells: &mut Cells, rules: &PlacementRules, rect: Rect, degrees: i32, entity: usize) {
    let n = cells.size as i32;
    let (w, h) = (rect.width(), rect.height());
    let fp = rules.footprint.as_bytes();
    for y in rect.y0..=rect.y1 {
        for x in rect.x0..=rect.x1 {
            let i = (n * y + x) as usize;
            let (set, owned, ty) = if fp.is_empty() {
                (true, true, rules.cell_type)
            } else {
                let c = fp
                    .get(footprint_index(x - rect.x0, y - rect.y0, w, h, degrees))
                    .copied()
                    .unwrap_or(b'.');
                (
                    c == b'#' || c == b'_',
                    c != b'.',
                    if c == b'_' { 2 } else { rules.cell_type },
                )
            };
            if set {
                cells.set_type(i, CellType(ty));
            }
            cells.objects[i] = owned.then_some(entity);
        }
    }
}

/// `FUN_1400a2830` (ZoneComponent activation, `FUN_1401356c0`): the entity becomes the
/// occupant (`+0x00`) of every non-`.` footprint cell; `.` cells count one more zone at `+0x28`.
/// Not ported: the path grid's blocked bit (citizens' mode 0 does not honour it).
pub fn zone(cells: &mut Cells, rules: &PlacementRules, rect: Rect, degrees: i32, entity: usize) {
    let n = cells.size as i32;
    let (w, h) = (rect.width(), rect.height());
    let fp = rules.footprint.as_bytes();
    for y in rect.y0..=rect.y1 {
        for x in rect.x0..=rect.x1 {
            let i = (n * y + x) as usize;
            let untouched = !fp.is_empty()
                && fp
                    .get(footprint_index(x - rect.x0, y - rect.y0, w, h, degrees))
                    .copied()
                    .unwrap_or(b'.')
                    == b'.';
            if untouched {
                cells.occupants[i] = None;
                cells.flags[i] = cells.flags[i].wrapping_add(1);
            } else {
                cells.occupants[i] = Some(entity);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn footprints_centre_on_the_cursor() {
        // Barn: 8 × 5.
        let r = footprint(256, (100, 100), 8, 5, 0).unwrap();
        assert_eq!((r.x0, r.x1, r.y0, r.y1), (97, 104, 98, 102));
        let r = footprint(256, (100, 100), 8, 5, 1).unwrap();
        assert_eq!((r.width(), r.height()), (5, 8));
        assert!(footprint(256, (0, 100), 3, 3, 0).is_none());
    }

    #[test]
    fn rotated_indices_cover_the_string() {
        // "ab cd" is 2 wide, 2 high (rows separated by one char, last row first).
        let (w, h) = (2, 2);
        let mut seen: Vec<usize> = Vec::new();
        for deg in [0, 90, 180, 270] {
            seen.clear();
            for j in 0..h {
                for i in 0..w {
                    seen.push(footprint_index(i, j, w, h, deg));
                }
            }
            seen.sort();
            assert_eq!(seen, vec![0, 1, 3, 4], "{deg}");
        }
    }
}
