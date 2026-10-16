//! BridgeTool (`DragTool`, vtable `0x140167f20`): a bridge dragged across water. Its start
//! (`FUN_140100c70`) keeps the template's CreateBridgeDescription; the drag's end follows the
//! longer axis from its start (`FUN_140100be0`); the check (slot `+0x70`, `FUN_1401001c0` →
//! `FUN_1401007a0`) wants bank, water, bank in a line with nothing in the way; the apply (slot
//! `+0x68`, `FUN_1400ff1a0`) places the bridge entity over the water with its parts (a start
//! piece at each end, pieces 8, 4, 2 or 1 long between). See docs/BRIDGES.md.

use banished_formats::game::{create_bridge_description, CreateBridgeDescription};
use banished_formats::pkg::Archive;
use banished_game::seeding::Placed;
use banished_game::terrain::HeightGrid;
use banished_game::town::cell_ground;

use super::{normalise, read, StartingTown};

/// The tool's template and description (`+0x38`) and the part templates (`+0x38..`).
#[derive(Debug, Clone)]
pub struct BridgeSpec {
    pub template: usize,
    pub desc: CreateBridgeDescription,
    pub parts: Vec<Option<usize>>,
}

/// A piece's length's part: the start piece (0), then 1, 2, 4 and 8 cells.
fn part_index(length: i32) -> usize {
    match length {
        1 => 1,
        2 => 2,
        4 => 3,
        8 => 4,
        _ => 0,
    }
}

/// The longest piece (8, 4, 2, 1) that fits `cells`; none for no cells.
fn piece(cells: i32) -> i32 {
    match cells {
        c if 8 <= c => 8,
        c if 4 <= c => 4,
        c if 2 <= c => 2,
        c if 0 < c => 1,
        _ => 0,
    }
}

/// `FUN_1400ff1a0`'s list of pieces for a bridge `length` cells long: a start piece (0, one
/// cell) at each end; between them the longest piece that fits the inside, then from the middle
/// outwards on each side pieces ever shorter.
pub fn pieces(length: i32) -> Vec<i32> {
    let inner = length - 2;
    let half = inner / 2;
    let middle = piece(inner);
    let mut left = Vec::new();
    let mut right = Vec::new();
    if 0 < inner {
        let mut rest = half - middle / 2;
        while 0 < rest {
            let p = piece(rest);
            left.push(p);
            rest -= p;
        }
        let mut at = middle / 2 + half;
        while at < inner {
            let p = piece(inner - at);
            right.push(p);
            at += p;
        }
    }
    let mut out = vec![0];
    out.extend(left.iter().rev());
    if 0 < inner {
        out.push(middle);
    }
    out.extend(right);
    out.push(0);
    out
}

impl StartingTown {
    /// `FUN_140100c70`: the template's CreateBridgeDescription and part templates.
    pub fn bridge_spec(&mut self, archive: &Archive, reference: &str) -> Option<BridgeSpec> {
        let key = normalise(reference);
        let desc =
            create_bridge_description(&read(archive, &key.replace(":resource", ":createbridge"))?)
                .map_err(|e| log::warn!("{reference}: {e}"))
                .ok()?;
        let template = self.load_template(archive, reference)?;
        let parts = desc
            .parts
            .iter()
            .map(|p| p.as_deref().and_then(|p| self.load_template(archive, p)))
            .collect();
        Some(BridgeSpec {
            template,
            desc,
            parts,
        })
    }

    /// `FUN_140100be0`: the drag's end on the row or column of its start, whichever the cursor
    /// is further along.
    pub fn bridge_end(&self, start: i32, cell: i32) -> i32 {
        let n = self.cells.size as i32;
        let (sx, sy) = (start % n, start / n);
        let (dx, dy) = (sx - cell % n, sy - cell / n);
        if dy.abs() <= dx.abs() {
            cell % n + n * sy
        } else {
            sx + n * (cell / n)
        }
    }

    /// `FUN_1401007a0`: over the rectangle from `a` to `b`, in rows: banks (`+0x24`), then the
    /// cells spanned (`+0x20`, at least one of `+0x2c`), then banks again, nothing standing on
    /// any. From the first spanned cell to one past the span, and `+0x34` to each side (of the
    /// `+0x30` types), no occupant, road or planned road. Returns whether it holds and the
    /// span's length.
    pub fn bridge_check(&self, spec: &BridgeSpec, a: i32, b: i32) -> (bool, i32) {
        let d = &spec.desc;
        let c = &self.cells;
        let n = c.size as i32;
        let bit = |i: i32| 1u32 << (c.types[i as usize].0 & 0x1f);
        let (x0, x1) = ((a % n).min(b % n), (a % n).max(b % n));
        let (y0, y1) = ((a / n).min(b / n), (a / n).max(b / n));
        let mut ok = true;
        let (mut state, mut before, mut span, mut after, mut needs) = (0, 0, 0, 0, false);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let i = n * y + x;
                if c.occupants[i as usize].is_some() {
                    ok = false;
                }
                let t = bit(i);
                match state {
                    0 if d.span & t != 0 => {
                        state = 1;
                        needs |= d.needs & t != 0;
                        span += 1;
                    }
                    0 if d.banks & t != 0 => before += 1,
                    1 if d.banks & t != 0 => {
                        state = 2;
                        after += 1;
                    }
                    1 if d.span & t != 0 => {
                        needs |= d.needs & t != 0;
                        span += 1;
                    }
                    2 if d.banks & t != 0 => after += 1,
                    _ => ok = false,
                }
            }
        }
        if !(state == 2 && before != 0 && span != 0 && after != 0) || !needs {
            ok = false;
        }
        // The first spanned cell, row by row.
        let first = (y0..=y1)
            .flat_map(|y| (x0..=x1).map(move |x| (x, y)))
            .find(|&(x, y)| d.span & bit(n * y + x) != 0);
        let (fx, fy) = first.unwrap_or((x0, y0));
        let ex = fx + if fx != x1 { span } else { 0 };
        let ey = fy + if fy != y1 { span } else { 0 };
        let w = d.half_width;
        for y in fy..=ey {
            for x in fx..=ex {
                for k in -w..=w {
                    let (cx, cy) = if fx == ex { (x + k, y) } else { (x, y + k) };
                    let i = n * cy + cx;
                    if i < 0 || n * n <= i {
                        ok = false;
                        continue;
                    }
                    let t = bit(i);
                    if k != 0 && d.sides & t == 0 {
                        ok = false;
                    }
                    if c.occupants[i as usize].is_some()
                        || t & 3 != 0
                        || c.road_kind[i as usize] != 0xff
                        || self.construction.pending_roads.contains_key(&i)
                    {
                        ok = false;
                    }
                }
            }
        }
        (ok, span)
    }

    /// `FUN_1400ff1a0`: a drag from `a` toward `b` places the bridge: the entity of the tool's
    /// template over the span (turned 90° when it runs along y), then each piece from the
    /// first spanned cell on (the first start piece turned about, the last as it is), each its
    /// MultiObject part, standing at water level between its end cells. Returns the entity.
    pub fn place_bridge(
        &mut self,
        archive: &Archive,
        spec: &BridgeSpec,
        a: i32,
        b: i32,
        terrain: &HeightGrid,
    ) -> Option<usize> {
        if a < 0 || b < 0 {
            return None;
        }
        let end = self.bridge_end(a, b);
        let (ok, length) = self.bridge_check(spec, a, end);
        if !ok {
            return None;
        }
        let list = pieces(length);
        let n = self.cells.size as i32;
        let (x0, x1) = ((a % n).min(end % n), (a % n).max(end % n));
        let (y0, y1) = ((a / n).min(end / n), (a / n).max(end / n));
        let types = &self.cells.types;
        let bit = |i: i32| 1u32 << (types[i as usize].0 & 0x1f);
        let (fx, fy) = (y0..=y1)
            .flat_map(|y| (x0..=x1).map(move |x| (x, y)))
            .find(|&(x, y)| spec.desc.span & bit(n * y + x) != 0)?;
        let along_x = (fx != x1) as i32;
        let along_y = (fy != y1) as i32;
        let w = spec.desc.half_width;
        let size = self.cells.size;
        let ground = |x: i32, y: i32| cell_ground(size, terrain, n * y + x);
        let (g0, g1) = (
            ground(fx, fy),
            ground(fx + (length - 1) * along_x, fy + (length - 1) * along_y),
        );
        let rect = (
            fx - w * along_y,
            fy - w * along_x,
            (along_x * length).max(w * along_y * 2 + 1),
            (along_y * length).max(w * along_x * 2 + 1),
        );
        let entity = self.placed.len();
        self.placed.push(Placed {
            template: spec.template,
            bucket: self.templates[spec.template].bucket,
            cell: (rect.0, rect.1),
            footprint: (rect.2, rect.3),
            position: [(g0[0] + g1[0]) * 0.5, (g0[1] + g1[1]) * 0.5, 0.0],
            yaw: (along_y * 90) as f32,
            variant: 0,
            mesh: usize::MAX,
            growth: 0.0,
            growth_state: None,
            spawn_state: None,
            alive: true,
            removal_requested: false,
            animation: None,
            scale: [1.0; 4],
        });
        let mut multi = Vec::new();
        let (mut x, mut y) = (fx, fy);
        let last = list.len() - 1;
        for (k, &p) in list.iter().enumerate() {
            let cells = p.max(1);
            let (w, h, mut yaw): (i32, i32, i32) = if along_x == 0 {
                (1, cells, 90)
            } else {
                (cells, 1, 0)
            };
            if k == 0 {
                yaw = if along_x == 0 { -90 } else { 180 };
            } else if k == last {
                yaw = if along_x == 0 { 90 } else { 0 };
            }
            let (g0, g1) = (ground(x, y), ground(x + w - 1, y + h - 1));
            let position = [(g0[0] + g1[0]) * 0.5, (g0[1] + g1[1]) * 0.5, 0.0];
            if let Some(t) = spec.parts.get(part_index(p)).copied().flatten() {
                let degrees = yaw.rem_euclid(360);
                let part = self.fence_part(t, (x, y, w, h), degrees, position);
                self.add_floor(part);
                multi.push(part);
            }
            x += cells * along_x;
            y += cells * along_y;
        }
        // `FUN_1400c9d70` of the entity last: its zone and its site.
        self.register_entity(entity);
        for cy in rect.1..rect.1 + rect.3 {
            for cx in rect.0..rect.0 + rect.2 {
                self.cells.occupants[(cy * n + cx) as usize] = Some(entity);
            }
        }
        self.add_site(archive, entity);
        if let Some(site) = self.construction.sites.get_mut(&entity) {
            site.parts = multi;
        }
        log::info!(
            "bridge {length} cells from ({fx}, {fy}): {:?}",
            list.iter().map(|p| p.max(&1)).collect::<Vec<_>>()
        );
        Some(entity)
    }

    /// FloorComponent activation (`FUN_140124810` → `FUN_140124860`): the part's floor mesh,
    /// turned and placed as its model, over its MapComponent's cells.
    fn add_floor(&mut self, part: usize) {
        let p = &self.placed[part];
        let Some(floor) = self.templates[p.template].floor.clone() else {
            return;
        };
        let (s, c) = p.yaw.to_radians().sin_cos();
        let at = p.position;
        let place = |v: [f32; 3]| {
            [
                c * v[0] - s * v[1] + at[0],
                s * v[0] + c * v[1] + at[1],
                v[2] + at[2],
            ]
        };
        let triangles = floor.iter().map(|t| t.map(place)).collect();
        let f = banished_game::floor::Floor {
            rect: (p.cell.0, p.cell.1, p.footprint.0, p.footprint.1),
            triangles,
        };
        let size = self.cells.size;
        self.cells.floors.add(size, part, f);
    }
}

#[cfg(test)]
mod tests {
    use super::pieces;

    #[test]
    fn bridge_pieces() {
        assert_eq!(pieces(2), vec![0, 0]);
        assert_eq!(pieces(5), vec![0, 2, 1, 0]);
        assert_eq!(pieces(9), vec![0, 1, 4, 2, 0]);
        assert_eq!(pieces(20), vec![0, 1, 4, 8, 4, 1, 0]);
        // The original's quirk: a span of three gets a piece too many.
        assert_eq!(pieces(3), vec![0, 1, 1, 0]);
    }
}
