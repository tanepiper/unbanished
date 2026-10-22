//! Workplace production: ResourceProduceComponent's states and the `ConsumeProduce` work of
//! ConsumeProduceComponent: the wood cutter, blacksmith, tailor, tavern, quarry and mine. See
//! docs/JOBS.md "Production".
//!
//! A worker of the workplace's profession takes the work at a free "use" point. In order it hauls
//! goods the current recipe does not use out of the building, fetches missing inputs, hauls the
//! workplace's own output piles away while its "create" points are full, and otherwise works one
//! cycle at its use point. Every `cycles` completed cycles the next cycle also consumes the inputs
//! and leaves the output as a pile on a free create point.
//!
//! Original: `FUN_140130700`.
mod cycle;
mod sequence;

// vtable 0x140167640

use crate::behaviour::character::{Action, MoveTo};
use crate::pathfind::WALKABLE;
use crate::town::{Haul, Sim};
use crate::work::construction::{ProduceWork, Recipe};
use crate::work::jobs::{Jobs, Work, WorkKind};

/// The stock-manager slot a raw material counts towards (production limits).
///
/// Original: `FUN_14008e460`.
pub fn production_group(flags: u32) -> Option<usize> {
    const ORDER: [(u32, usize); 21] = [
        (10, 0),
        (11, 1),
        (12, 2),
        (7, 3),
        (8, 9),
        (9, 4),
        (5, 5),
        (13, 6),
        (14, 7),
        (17, 8),
        (16, 10),
        (18, 11),
        (19, 12),
        (20, 13),
        (21, 14),
        (22, 15),
        (23, 16),
        (24, 17),
        (25, 18),
        (26, 19),
        (27, 20),
    ];
    ORDER
        .iter()
        .find(|(bit, _)| flags >> bit & 1 != 0)
        .map(|&(_, g)| g)
}

/// What the try decided before offering the output piles' pickups ([`Sim::produce_resume`]).
#[derive(Debug, Clone, Copy)]
pub(crate) struct ProduceResume {
    /// The recipe selected at the start of the try (its output is the one produced).
    selected: usize,
    slot: usize,
    point: Option<(usize, i32)>,
    all_inputs: bool,
}

impl Sim<'_, '_> {
    fn recipes(&self, e: usize) -> Option<&crate::work::construction::ProduceTemplate> {
        let site = self.sites.get(&e)?;
        self.site_templates[site.template].produce.as_ref()
    }

    /// `FUN_14011e940`: the selected recipe; selector −1 draws `RandFloat` over the weights.
    pub(crate) fn produce_select(&mut self, e: usize) -> usize {
        let Some(state) = self.sites.get(&e).and_then(|s| s.produce.as_ref()) else {
            return 0;
        };
        if state.selector >= 0 {
            return state.selector as usize;
        }
        if state.selector != -1 {
            return 0;
        }
        let weights = state.weights.clone();
        let total: f32 = weights.iter().sum();
        let r = self.rng.rand_float();
        let mut acc = 0.0;
        for (i, w) in weights.iter().enumerate() {
            acc += w;
            if r * total < acc {
                return i;
            }
        }
        0
    }

    /// `FUN_1401282f0(interact, 0, i)`: use point `i` in the world and its heading: the
    /// point's direction (`FromYawPitch(yaw, 0)` at load, `FUN_1401289d0`) turned by the
    /// entity transform, then `Heading`. The entity's own points come first, then its
    /// MultiObject parts' in turn, each turned by its own transform; past them all, the
    /// entity's origin.
    pub(crate) fn use_point(&self, e: usize, i: usize) -> ([f32; 3], f32) {
        let mut i = i;
        for x in self.interact_entities(e) {
            let p = &self.placed[x];
            let t = &self.templates[p.template];
            if i < t.use_points.len() {
                let yaw = t.use_yaws.get(i).copied().unwrap_or(0.0);
                let model = p.model(t);
                return (model.apply(t.use_points[i]), model.heading_of(yaw));
            }
            i -= t.use_points.len();
        }
        let p = &self.placed[e];
        let model = p.model(&self.templates[p.template]);
        (model.apply([0.0; 3]), model.heading_of(0.0))
    }

    /// `FUN_140128540(interact, 0)`: the use points of the entity and its MultiObject parts.
    pub(crate) fn use_point_count(&self, e: usize) -> usize {
        self.interact_entities(e)
            .iter()
            .map(|&x| self.templates[self.placed[x].template].use_points.len())
            .sum()
    }

    /// The cell of create point `i` (`FUN_1401282f0(interact, 2, i)`).
    pub(crate) fn create_cell(&self, e: usize, i: usize) -> i32 {
        // The entity's own create points, then its MultiObject parts' (`FUN_1401282f0`).
        let mut i = i;
        for x in self.interact_entities(e) {
            let points = &self.templates[self.placed[x].template].create_points;
            if i < points.len() {
                return self.cell_of(self.world_point(x, points[i]));
            }
            i -= points.len();
        }
        self.cell_of(self.world_point(e, [0.0; 3]))
    }

    /// ConsumeProduceComponent slot `+0x90` (`FUN_14011e1d0`) and the work's constructor
    /// (`FUN_1400ee6d0`): one worker slot per use point, one state per create point.
    pub(crate) fn produce_work(&self, e: usize) -> Work {
        let t = &self.templates[self.placed[e].template];
        let profession = self
            .workforce
            .workplaces
            .get(&e)
            .and_then(|w| w.profession)
            .unwrap_or(self.professions.laborer);
        Work::new(
            e,
            self.rect_cell(e),
            profession,
            WorkKind::Produce(ProduceWork {
                workers: vec![None; t.use_points.len()],
                points: vec![0; t.create_points.len()],
                full: false,
                cycles: 0,
            }),
        )
    }

    /// `FUN_1401307e0`: switch production off (`+0x48`) and go idle (0xe).
    pub(crate) fn produce_disable(&mut self, e: usize) {
        let Some(site) = self.sites.get_mut(&e) else {
            return;
        };
        if let Some(p) = &mut site.produce {
            p.disabled = true;
        }
        let m = &site.machine;
        let state = if m.pending < 0 { m.state } else { m.pending };
        if state != 0xe {
            site.machine.request(0xe);
        }
    }

    /// `FUN_140130460` (transition 2 → 0xd): every create point's cell becomes ground with no
    /// object, ready for output piles.
    pub(crate) fn produce_clear_points(&mut self, e: usize) {
        // `FUN_140128540(.., 2)`: the entity's and its MultiObject parts' create points.
        let n: usize = self
            .interact_entities(e)
            .iter()
            .map(|&x| self.templates[self.placed[x].template].create_points.len())
            .sum();
        for i in 0..n {
            let cell = self.create_cell(e, i) as usize;
            self.cells.set_type(cell, crate::terrain::CellType(2));
            self.cells.objects[cell] = None;
        }
    }

    /// `FUN_1400a1210(map, cell, 1, owner, mask)`: the nearest cell, in rings of radius 0..=6
    /// (rows, then columns), with no object, no occupant (or `owner`), not water, no road
    /// (built or planned), whose type is in `mask`.
    pub(crate) fn free_cell(&self, cell: i32, owner: Option<usize>, mask: u32) -> Option<i32> {
        self.free_cell_where(cell, owner, |t, _| mask >> (t & 0x1f) & 1 != 0)
    }

    /// `FUN_1400a1210(map, cell, 0, none, _)` (a loose pile's cell, `FUN_1400ccd90`): as
    /// [`Sim::free_cell`], on ground (type 2) with no occupant, no flags, and the terrain's
    /// normal at the cell's middle within `0.97` of up.
    pub(crate) fn free_flat_cell(&self, cell: i32) -> Option<i32> {
        let n = self.size as i32;
        self.free_cell_where(cell, None, |t, i| {
            if t != 2 || self.cells.occupants[i].is_some() || self.cells.flags[i] != 0 {
                return false;
            }
            let half = (n >> 1) as f32 - 0.5;
            let (x, y) = ((i as i32 % n) as f32 - half, (i as i32 / n) as f32 - half);
            let mut normal = [0.0; 3];
            self.cells
                .floors
                .ground_normal(self.size, self.terrain, x, y, &mut normal);
            0.97 <= normal[2]
        })
    }

    fn free_cell_where(
        &self,
        cell: i32,
        owner: Option<usize>,
        ok: impl Fn(u32, usize) -> bool,
    ) -> Option<i32> {
        let n = self.size as i32;
        let (cx, cy) = (cell % n, cell / n);
        for r in 0..=6 {
            for y in cy - r..=cy + r {
                for x in cx - r..=cx + r {
                    if (y - cy).abs() != r && (x - cx).abs() != r {
                        continue;
                    }
                    if x < 0 || y < 0 || x >= n || y >= n {
                        continue;
                    }
                    let i = (y * n + x) as usize;
                    if self.cells.objects[i].is_some() {
                        continue;
                    }
                    if let Some(o) = self.cells.occupants[i] {
                        if Some(o) != owner {
                            continue;
                        }
                    }
                    let t = self.cells.types[i].0 as u32;
                    // Types 0 and 1 (`1 << t & 3`), a road (`+0x2a`) or a planned one.
                    if 1u32 << (t & 0x1f) & 3 != 0
                        || self.cells.road_kind[i] != 0xff
                        || self.pending_roads.contains_key(&(i as i32))
                    {
                        continue;
                    }
                    if ok(t, i) {
                        return Some(y * n + x);
                    }
                }
            }
        }
        None
    }
}

/// `FUN_1400c4440`'s arguments.
struct ProduceSequence {
    target: [f32; 3],
    facing: f32,
    recipe: Recipe,
    produce: bool,
    out_cell: i32,
    work_type: u32,
    duration: f32,
    param: i32,
}
