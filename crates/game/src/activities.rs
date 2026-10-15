//! What each CitizenAI activity sets in motion (`FUN_1401146c0`'s targets). See
//! docs/CITIZEN-AI.md. Only the parts of the town the activities query are modelled here, in
//! [`TownView`].

use crate::pathfind::{WALKABLE, WATER};
use crate::random::Random;
use crate::terrain::CellType;

/// `MapComponent +0x28..+0x2e`: an entity's map rectangle `(x, y, w, h)`.
pub type MapRect = (i32, i32, i32, i32);

/// What activities need to know about one entity.
#[derive(Debug, Clone, Default)]
pub struct EntityView {
    pub alive: bool,
    /// Has a MapComponent: its rectangle.
    pub rect: Option<MapRect>,
    /// InteractComponent "use" points in world space (category 0).
    pub use_points: Vec<[f32; 3]>,
    /// A HappinessComponent usable for loitering (`FUN_14004b080`: description type != 5 and
    /// enabled): the description's loiter padding `+0x2c`.
    pub happiness: Option<i32>,
}

/// The map and entities activities read.
pub struct TownView<'a> {
    pub size: usize,
    pub types: &'a [CellType],
    pub regions: &'a [u16],
    /// `+0x00` occupant and `+0x08` object per cell.
    pub occupants: &'a [Option<usize>],
    pub objects: &'a [Option<usize>],
    pub entities: &'a [EntityView],
    /// The town manager's HappinessComponent registry (`Game +0xf0`, `+0x50`).
    pub happiness: &'a [usize],
}

impl TownView<'_> {
    pub fn cell_of(&self, p: [f32; 3]) -> i32 {
        let n = self.size as i32;
        let half = (n >> 1) as f32;
        let x = ((p[0] + half) as i32).clamp(0, n - 1);
        let y = ((p[1] + half) as i32).clamp(0, n - 1);
        n * y + x
    }

    /// `FUN_1400a55c0`: can a walker with `mask` get from `a` to `b` (water types skip the
    /// region test)?
    pub fn reachable(&self, mask: u32, a: i32, b: i32) -> bool {
        let (ba, bb) = (
            1u32 << (self.types[a as usize].0 & 0x1f),
            1u32 << (self.types[b as usize].0 & 0x1f),
        );
        let mask = mask | ba | bb;
        if mask & ba == 0 || mask & bb == 0 {
            return false;
        }
        if mask & WATER != 0 {
            return true;
        }
        let (ra, rb) = (self.regions[a as usize], self.regions[b as usize]);
        ra != 0 && rb != 0 && ra == rb
    }

    /// `FUN_14008d480(entity, 0)`: the cell a citizen walks to. With use points, the first
    /// (the original draws `Rand(count)` when there are several but always takes point 0);
    /// otherwise the centre of the map rectangle.
    pub fn entrance(&self, entity: usize, rng: &mut Random) -> i32 {
        let e = &self.entities[entity];
        if !e.use_points.is_empty() {
            if e.use_points.len() > 1 {
                rng.rand_below(e.use_points.len() as u32);
            }
            return self.cell_of(e.use_points[0]);
        }
        match e.rect {
            Some((x, y, w, h)) => (y + (h >> 1)) * self.size as i32 + (w >> 1) + x,
            None => -1,
        }
    }

    /// `FUN_14011ada0`: is `entity` reachable from `cell`?
    pub fn can_reach(&self, cell: i32, entity: usize, rng: &mut Random) -> bool {
        let e = self.entrance(entity, rng);
        e >= 0 && self.reachable(WALKABLE, cell, e)
    }

    /// Object on a cell as the scans read it: the occupant, else the stamped object.
    fn object(&self, cell: usize) -> Option<usize> {
        self.occupants[cell].or(self.objects[cell])
    }
}

/// A loiter (`FUN_14010fa30` → `FUN_1400bfb80`): walk to `target`, idle for `duration`
/// seconds, then notify `happiness` (when set).
#[derive(Debug, Clone, PartialEq)]
pub struct Loiter {
    pub target: [f32; 3],
    pub duration: f32,
    pub happiness: Option<usize>,
}

/// `FUN_14010fa30` (activity 12): choose a spot near something to stand at. `cell` is the
/// citizen's cell, `home` its home entity, `range` `CitizenDescription +0xbc`.
/// `ground(cell)` returns a cell centre on the ground.
pub fn loiter(
    town: &TownView,
    cell: i32,
    home: Option<usize>,
    range: i32,
    rng: &mut Random,
    ground: impl Fn(i32) -> [f32; 3],
) -> Option<Loiter> {
    let n = town.size as i32;
    let home_cell = home.map_or(-1, |h| town.entrance(h, rng));
    let mut chosen = town.occupants[cell as usize];
    if rng.rand_float() < 0.25 {
        chosen = None;
    }
    // The occupant only counts when it is a usable happiness entity reachable from here
    // through its first use point.
    chosen = chosen.filter(|&e| {
        let v = &town.entities[e];
        v.happiness.is_some()
            && v.use_points
                .first()
                .is_some_and(|&p| town.reachable(WALKABLE, cell, town.cell_of(p)))
    });
    if chosen.is_none() {
        chosen = nearest_happiness(town, cell, range, rng);
        if chosen.is_none() && home_cell >= 0 {
            chosen = nearest_happiness(town, home_cell, range, rng);
        }
    }
    let mut pad = chosen.and_then(|e| town.entities[e].happiness).unwrap_or(6);
    if chosen.is_none() {
        // Scan 13 x 13 cells around the citizen (rows outer) for reachable map objects, then
        // the home; pick one from the set. The original orders the set by object address; the
        // port uses creation order.
        let mut set = std::collections::BTreeSet::new();
        let (cx, cy) = (cell % n, cell / n);
        for y in cy - 6..=cy + 6 {
            for x in cx - 6..=cx + 6 {
                if x < 0 || x >= n || y < 0 || y >= n {
                    continue;
                }
                let Some(e) = town.object((n * y + x) as usize) else {
                    continue;
                };
                if town.entities[e].rect.is_some() && town.can_reach(cell, e, rng) {
                    set.insert(e);
                }
            }
        }
        pad = 6;
        if let Some(h) = home {
            if town.can_reach(cell, h, rng) {
                set.insert(h);
            }
        }
        if !set.is_empty() {
            let k = rng.rand_below(set.len() as u32) as usize;
            chosen = set.iter().nth(k).copied();
        }
    }
    let (happy, (x0, y0, w, h)) = match chosen {
        None => (None, (cell % n, cell / n, 1, 1)),
        Some(e) => {
            let v = &town.entities[e];
            (
                v.happiness.map(|_| e),
                v.rect.unwrap_or((cell % n, cell / n, 1, 1)),
            )
        }
    };
    // Walkable ground (types 0-2) in the rectangle grown by `pad` and reachable from here,
    // as an ordered set of cell indices.
    let mut cells = std::collections::BTreeSet::new();
    for y in y0 - pad..y0 - pad + h + 2 * pad {
        for x in x0 - pad..x0 - pad + w + 2 * pad {
            if x < 0 || x >= n || y < 0 || y >= n {
                continue;
            }
            let c = n * y + x;
            if town.types[c as usize].0 < 3 && town.reachable(WALKABLE, cell, c) {
                cells.insert(c);
            }
        }
    }
    if cells.is_empty() {
        return None;
    }
    let k = rng.rand_below(cells.len() as u32) as usize;
    let spot = *cells.iter().nth(k).unwrap();
    let base = ground(spot);
    // `DAT_14017880c` = 0.33; the y offset is drawn first.
    let dy = rng.rand_float_tol() * 0.33;
    let dx = rng.rand_float_tol() * 0.33;
    // `DAT_140178898` = 5, `DAT_1401788b0` = 10.
    let duration = rng.rand_float_tol() * 5.0 + 10.0;
    Some(Loiter {
        target: [base[0] + dx, base[1] + dy, base[2]],
        duration,
        happiness: happy,
    })
}

/// `FUN_14008eea0`: a random registered HappinessComponent owner within `2 * range` of `cell`
/// (by path rank) and reachable. Path ranks (`FUN_1400a4ce0`) are not ported; with an empty
/// registry (New Game) this draws nothing.
fn nearest_happiness(town: &TownView, _cell: i32, _range: i32, _rng: &mut Random) -> Option<usize> {
    if town.happiness.is_empty() {
        return None;
    }
    // Not ported: path-rank distance and the random pick among candidates.
    None
}
