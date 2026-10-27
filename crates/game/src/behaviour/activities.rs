//! What each activity of the citizens' AI does. See docs/CITIZEN-AI.md. Only the parts of the town
//! that the activities ask about are modelled here, in [`TownView`].
//!
//! Original: the activities run by `FUN_1401146c0`.

use crate::pathfind::{WALKABLE, WATER};
use crate::random::Random;
use crate::sectors::SectorGraph;
use crate::terrain::CellType;
use crate::work::stock::StorageSearchMap;

// MapComponent +0x28..+0x2e
/// `MapComponent`: an entity's map rectangle `(x, y, w, h)`.
pub type MapRect = (i32, i32, i32, i32);

/// What activities need to know about one entity.
#[derive(Debug, Clone, Default)]
pub struct EntityView {
    /// Whether it is still in the world.
    pub alive: bool,
    /// Has a MapComponent: its rectangle.
    pub rect: Option<MapRect>,
    /// InteractComponent "use" points in world space (category 0).
    pub use_points: Vec<[f32; 3]>,
    /// Whether it has an InteractComponent.
    pub has_interact: bool,
    /// The position.
    pub position: [f32; 3],
    /// Whether it has a model.
    pub has_model: bool,
    // +0x2c; usable when FUN_14004b080 finds description type != 5 and enabled.
    /// For a building where citizens may loiter (a happiness component): the description's loiter
    /// padding.
    pub happiness: Option<i32>,
}

/// The geometry called by the town's storage search. Uses the same sector cache as the AI.
pub struct StorageGeometry<'a, 'b> {
    /// A view of the town.
    pub town: &'a TownView<'b>,
    /// The sector graph.
    pub sectors: &'a mut SectorGraph,
    /// The ground point of a cell.
    pub ground: &'a dyn Fn(i32) -> [f32; 3],
}

impl StorageSearchMap for StorageGeometry<'_, '_> {
    fn entrance(&mut self, entity: usize, rng: &mut Random) -> i32 {
        self.town.entrance(entity, rng)
    }

    /// FUN_140008830: path rank, or map width plus truncated Euclidean cell distance.
    fn rank(&mut self, a: i32, b: i32) -> i32 {
        let n = self.town.size as i32;
        self.sectors.rebuild(self.town.types);
        let d = self.sectors.distance(
            a,
            b,
            self.town.reachable(WALKABLE, a, b),
            true,
            self.town.types,
        );
        if d >= 0 {
            d
        } else {
            let (dx, dy) = ((a % n - b % n) as f32, (a / n - b / n) as f32);
            n + (dx * dx + dy * dy).sqrt() as i32
        }
    }

    /// Port of FUN_14004f710: draw a use point (including Rand(1)), then test reachability.
    fn market_position(
        &mut self,
        entity: usize,
        from: i32,
        rng: &mut Random,
    ) -> Option<(i32, [f32; 3])> {
        let points = &self.town.entities[entity].use_points;
        let k = rng.rand_below(points.len() as u32) as usize;
        let p = *points.get(k)?;
        let cell = self.town.cell_of(p);
        self.town
            .reachable(WALKABLE, from, cell)
            .then(|| (self.rank(from, cell), p))
    }

    /// Port of FUN_140132aa0: nearest reachable use point, or scan the grown map rectangle.
    /// The rectangle branch includes one extra row at its upper end, as the x64 loop does.
    fn storage_position(&mut self, entity: usize, from: i32) -> Option<(i32, [f32; 3])> {
        let e = &self.town.entities[entity];
        let mut best = i32::MAX;
        let mut target = None;
        if e.has_interact {
            for p in &e.use_points {
                let cell = self.town.cell_of(*p);
                if self.town.reachable(WALKABLE, from, cell) {
                    let d = self.rank(from, cell);
                    if d >= 0 && d < best {
                        best = d;
                        target = Some(*p);
                    }
                }
            }
        } else if let Some((x, y, w, h)) = e.rect {
            let n = self.town.size as i32;
            for cy in y - 1..=y + h + 1 {
                for cx in x - 1..x + w + 1 {
                    if cx < 0 || cy < 0 || cx >= n || cy >= n {
                        continue;
                    }
                    let cell = cy * n + cx;
                    if (1u32 << (self.town.types[cell as usize].0 & 31)) & 0xe07 != 0
                        && self.town.reachable(WALKABLE, from, cell)
                    {
                        let d = self.rank(cell, from);
                        if d < best {
                            best = d;
                            target = Some((self.ground)(cell));
                        }
                    }
                }
            }
        }
        target.map(|p| (best, p))
    }
}

/// The map and entities activities read.
pub struct TownView<'a> {
    /// The map's size in cells.
    pub size: usize,
    /// The type of each cell.
    pub types: &'a [CellType],
    /// The walkable region of each cell.
    pub regions: &'a [u16],
    // +0x00; +0x08
    /// The occupant of each cell.
    pub occupants: &'a [Option<usize>],
    /// The object on each cell.
    pub objects: &'a [Option<usize>],
    // +0x2d
    /// The bridges' floors (and the floor scene), for the ground under a point.
    pub floors: &'a crate::world::floor::Floors,
    /// A view of each entity.
    pub entities: &'a [EntityView],
    // +0x50; Game +0xf0
    /// The town manager's HappinessComponent registry (`Game`).
    pub happiness: &'a [usize],
}

impl TownView<'_> {
    /// The map cell that holds a position.
    pub fn cell_of(&self, p: [f32; 3]) -> i32 {
        let n = self.size as i32;
        let half = (n >> 1) as f32;
        let x = ((p[0] + half) as i32).clamp(0, n - 1);
        let y = ((p[1] + half) as i32).clamp(0, n - 1);
        n * y + x
    }

    /// Can a walker with `mask` get from `a` to `b` (water types skip the region test)?
    ///
    /// Original: `FUN_1400a55c0`.
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

    /// The cell a citizen walks to at an entity: its first use point (the original draws
    /// `Rand(count)` when there are several, but always takes point 0), else the centre of its map
    /// rectangle.
    ///
    /// Original: `FUN_14008d480(entity, 0)`.
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

    /// `Interact::PointPosition(entity, 0)` as a cell (no draw): the first use point, else
    /// the map rectangle's centre.
    pub fn point_cell(&self, entity: usize) -> i32 {
        let e = &self.entities[entity];
        if let Some(p) = e.use_points.first() {
            return self.cell_of(*p);
        }
        match e.rect {
            Some((x, y, w, h)) => (y + (h >> 1)) * self.size as i32 + (w >> 1) + x,
            None => -1,
        }
    }

    /// Is `entity` reachable from `cell`?
    ///
    /// Original: `FUN_14011ada0`.
    pub fn can_reach(&self, cell: i32, entity: usize, rng: &mut Random) -> bool {
        let e = self.entrance(entity, rng);
        e >= 0 && self.reachable(WALKABLE, cell, e)
    }

    /// Object on a cell as the scans read it: the occupant, else the stamped object.
    fn object(&self, cell: usize) -> Option<usize> {
        self.occupants[cell].or(self.objects[cell])
    }
}

/// The workplace searches' view of the town ([`crate::work::professions::Search`]).
pub struct TownSearch<'a, 'b> {
    /// A view of the town.
    pub town: &'a TownView<'b>,
    /// A citizen's map cell.
    pub cell: &'a dyn Fn(usize) -> i32,
    /// A profession works anywhere.
    pub anywhere: &'a dyn Fn(usize) -> bool,
    /// A workplace's work priority.
    pub priority: &'a dyn Fn(usize) -> i32,
    /// The game's random numbers.
    pub rng: &'a mut Random,
}

impl crate::work::professions::Search for TownSearch<'_, '_> {
    fn cell(&self, c: usize) -> i32 {
        (self.cell)(c)
    }
    fn anywhere(&self, profession: usize) -> bool {
        (self.anywhere)(profession)
    }
    fn priority(&self, workplace: usize) -> i32 {
        (self.priority)(workplace)
    }
    fn reach(&mut self, cell: i32, workplace: usize) -> bool {
        self.town.can_reach(cell, workplace, self.rng)
    }
    fn rng(&mut self) -> &mut Random {
        self.rng
    }
}

/// A loiter: walk to `target`, idle for `duration` seconds, then notify `happiness` (when set).
///
/// Original: `FUN_14010fa30`, `FUN_1400bfb80`.
#[derive(Debug, Clone, PartialEq)]
pub struct Loiter {
    /// Where to walk.
    pub target: [f32; 3],
    /// How long to idle, in seconds.
    pub duration: f32,
    /// The building to notify after, if any.
    pub happiness: Option<usize>,
}

// CitizenDescription +0xbc
/// Activity 12: choose a spot near something to stand at. `cell` is the citizen's cell, `home` its
/// home, `range` the citizen description's range. `ground(cell)` returns a cell's centre on the
/// ground; `rank(a, b)` is the path-rank distance.
///
/// Original: `FUN_14010fa30`; the distance is `FUN_140008830`.
pub fn loiter(
    town: &TownView,
    cell: i32,
    home: Option<usize>,
    range: i32,
    rng: &mut Random,
    ground: impl Fn(i32) -> [f32; 3],
    rank: &mut dyn FnMut(i32, i32) -> i32,
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
        chosen = nearest_happiness(town, cell, range, rng, rank);
        if chosen.is_none() && home_cell >= 0 {
            chosen = nearest_happiness(town, home_cell, range, rng, rank);
        }
    }
    let mut pad = chosen.and_then(|e| town.entities[e].happiness).unwrap_or(6);
    if chosen.is_none() {
        // Scan 13 x 13 cells around the citizen (rows outer) for reachable map objects, then
        // the home; pick one from the set.
        // Deviation: the original's set is a tree keyed by each object's `Object::Internal`
        // address, which depends on the allocator's history; the port uses creation order.
        // Measured on the CrossOver pair (docs/ITERATION-ORDER.md): the pick differs at draw 815.
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
            // A comparison run may give the original's pick (`super::loiter_order`).
            let k = super::loiter_order::place(k, set.iter().map(|&e| town.entities[e].rect));
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

/// `FUN_14008eea0`: a random registered HappinessComponent owner (not the unhappy kind) whose
/// first use point is within path rank `2 * range` of `cell` and reachable, in registry
/// order, picked with `Rand(count)`. Nothing is drawn without candidates.
fn nearest_happiness(
    town: &TownView,
    cell: i32,
    range: i32,
    rng: &mut Random,
    rank: &mut dyn FnMut(i32, i32) -> i32,
) -> Option<usize> {
    let mut found = Vec::new();
    for &e in town.happiness {
        let point = town.point_cell(e);
        if point < 0 {
            continue;
        }
        if rank(cell, point) < range * 2
            && town.reachable(WALKABLE, cell, point)
            && town.entities[e].happiness.is_some()
        {
            found.push(e);
        }
    }
    if found.is_empty() {
        return None;
    }
    Some(found[rng.rand_below(found.len() as u32) as usize])
}
