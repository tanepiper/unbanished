//! The forester's lodge and its work. See docs/JOBS.md, "Foresters".
//!
//! Like the gatherer, a worker first takes the building's own piles (AutoPickup), then draws random
//! cells within the radius. An empty cell in the entrance's region plants a sapling (while planting
//! is on); a grown, unclaimed natural resource whose raw material is neither food nor herbs is cut
//! (while cutting is on and the logs are under their limit).
// vtable 0x140169890; vtable 0x1401677c0; +0x90

use crate::behaviour::character::{Action, MoveTo};
use crate::nature::seeding::Seeder;
use crate::pathfind::{nearest_neighbour, WALKABLE};
use crate::production::herbalist::HerbalistWork;
use crate::scheduler::ticks;
use crate::town::Sim;
use crate::work::jobs::{Work, WorkKind};

/// A resolved ForesterDescription.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ForesterTemplate {
    // +0x20
    /// The tree it plants (index into the placed-object templates).
    pub tree: usize,
    // +0x28
    /// The stock-manager slot whose limit stops the cutting.
    pub group: i32,
}

// +0x50; +0x51
/// ForesterComponent plant and cut (the lodge's two toggle buttons).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Forester {
    /// Whether it plants trees.
    pub plant: bool,
    /// Whether it cuts trees.
    pub cut: bool,
}

impl Sim<'_, '_> {
    /// Slot `+0x90` (`FUN_140124c50`): outside the idle state (0xe), a lodge with both
    /// toggles off gets both on; then the work.
    pub(crate) fn forester_work(&mut self, e: usize) -> Work {
        if let Some(site) = self.sites.get_mut(&e) {
            let m = &site.machine;
            let state = if m.pending < 0 { m.state } else { m.pending };
            if let Some(f) = &mut site.forester {
                if state != 0xe && !f.plant && !f.cut {
                    *f = Forester {
                        plant: true,
                        cut: true,
                    };
                }
            }
        }
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
            WorkKind::Forester(HerbalistWork::default()),
        )
    }

    /// Slot `+0x20` (`FUN_1400f0390`) after the AutoPickup offers.
    pub(crate) fn forester_try(&mut self, work: &mut Work, c: usize) -> u8 {
        let e = work.entity;
        let Some(site) = self.sites.get(&e) else {
            return 0;
        };
        let toggles = site.forester.unwrap_or_default();
        let t = &self.site_templates[site.template];
        let Some(desc) = t.forester.clone() else {
            return 0;
        };
        let radius = t.radius.unwrap_or(0);
        let group = desc.group.clamp(0, 20) as usize;
        let (count, limit) = self.stock[group];
        self.set_icon(e, crate::work::professions::icon::LIMIT, limit <= count);
        let WorkKind::Forester(h) = &work.kind else {
            return 0;
        };
        if self.work_max(e) < h.workers.len() as i32 {
            return 1;
        }
        let n = self.size as i32;
        let r2 = radius * radius;
        let attempts = (r2 as f32 * std::f32::consts::PI).sqrt() as i32;
        let (cx, cy) = (work.cell % n, work.cell / n);
        let from = self.citizen_cell(c);
        let entrance = self.entrance_cell(e);
        let in_map = |cell: i32| 0 <= cell && cell < n * n;
        for _ in 0..attempts {
            let (x, y) = loop {
                let (dx, dy) = loop {
                    let dx = self.rng.rand_tol(radius);
                    let dy = self.rng.rand_tol(radius);
                    if dx * dx + dy * dy <= r2 {
                        break (dx, dy);
                    }
                };
                // As in the gatherer's, only the row is kept on the map.
                let (x, y) = (cx + dx, cy + dy);
                if 0 <= y && y < n {
                    break (x, y);
                }
            };
            let cell = n * y + x;
            // Deviation: cells off the map's edge (and an entrance off the map) read outside
            // the cell array in the original; no candidate here.
            if !in_map(cell) || !in_map(entrance) {
                continue;
            }
            let (ci, ei) = (cell as usize, entrance as usize);
            let bit = |ty: u8| 1u32 << (ty & 0x1f);
            let (cb, eb) = (bit(self.cells.types[ci].0), bit(self.cells.types[ei].0));
            if cb & 0x1e0 != 0 {
                continue;
            }
            // `FUN_1400a1950` relabels the regions first; the port keeps them current.
            let (rc, re) = (self.regions[ci], self.regions[ei]);
            let joined = (cb | eb) & 0x1e0 != 0 || (rc != 0 && re != 0 && rc == re);
            if !joined || self.cells.occupants[ci].is_some() {
                continue;
            }
            if self.geometry_rank(entrance, cell) > radius * 2 {
                continue;
            }
            let object = self.cells.objects[ci];
            if toggles.plant && object.is_none() {
                if !self.reachable(cell, from) {
                    continue;
                }
                let admits = crate::world::placement::cell_admits(
                    self.cells,
                    self.terrain,
                    x,
                    y,
                    4,
                    0.97,
                    1000.0,
                    0.0,
                    false,
                    false,
                    |_| false,
                );
                if !admits || !self.seeder().density_ok(desc.tree, x, y) {
                    continue;
                }
                let point = self.ground_at(cell);
                self.plant_sequence(c, work, desc.tree, point, cell, None, None, 0);
                return self.forester_took(work, c);
            }
            let Some(o) = object else { continue };
            if !toggles.cut {
                continue;
            }
            let template = self.placed[o].template;
            if self.templates[template].natural.is_none() {
                continue;
            }
            let (count, limit) = self.stock[group];
            if count >= limit {
                continue;
            }
            let flags = self.templates[template]
                .pile
                .and_then(|p| self.templates[p].pile_material)
                .map_or(0, |m| self.materials[m].flags);
            if flags & 0x20 != 0 || flags & 0x2000 != 0 {
                continue;
            }
            if self.placed[o].growth < 1.0 || self.claimed.contains(&o) {
                continue;
            }
            let Some(nb) = nearest_neighbour(self.size, &self.cells.types, cell, from, 0xe07)
            else {
                continue;
            };
            if !self.reachable(nb, from) {
                continue;
            }
            let point = self.ground_at(nb);
            self.removal_sequence(c, work, o, point, true);
            return self.forester_took(work, c);
        }
        let misses = match &work.kind {
            WorkKind::Forester(h) => h.misses,
            _ => 0,
        };
        if attempts < misses {
            if self.states[c].carried.used(self.materials) < 1 {
                return 0;
            }
            self.drop_off(c);
            return 2;
        }
        self.wait_sequence(c, work, 0);
        self.took_job(c, work);
        if let WorkKind::Forester(h) = &mut work.kind {
            h.workers.insert(c);
            h.misses += 1;
        }
        2
    }

    fn forester_took(&mut self, work: &mut Work, c: usize) -> u8 {
        self.took_job(c, work);
        if let WorkKind::Forester(h) = &mut work.kind {
            h.misses = 0;
            h.workers.insert(c);
        }
        2
    }

    /// `FUN_1400c3bb0`: take the tool out, walk to the cell, work 2.1 s, plant (the cell must
    /// have `occupant`), put the tool away, send `message`, report `param`. The forester has no
    /// occupant or message; an orchard has itself and its message 0.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn plant_sequence(
        &mut self,
        c: usize,
        work: &mut Work,
        tree: usize,
        point: [f32; 3],
        cell: i32,
        occupant: Option<usize>,
        message: Option<crate::behaviour::character::Message>,
        param: i32,
    ) {
        self.clear_citizen(c, Some(work));
        self.on_empty_think(c);
        self.push(
            c,
            Action::CompleteTask {
                work: work.id,
                param,
            },
        );
        if let Some(m) = message {
            self.push(c, Action::SendMessage(m));
        }
        self.push(c, Action::ShowTool(-1));
        self.push(
            c,
            Action::CreateNaturalResource {
                template: tree,
                occupant,
                cell,
            },
        );
        self.push(c, self.perform(c, -1000.0, 0xc, 2.1, true));
        let walk = MoveTo::new(point, 3, 0, WALKABLE, 0, -1.0, self.rng);
        self.push(c, Action::move_to(walk));
        self.push(c, Action::ShowTool(3));
    }

    /// `FUN_1400c7960(citizen, work, param)`: stand 0.25 s, then report `param`.
    pub(crate) fn wait_sequence(&mut self, c: usize, work: &mut Work, param: i32) {
        self.clear_citizen(c, Some(work));
        self.on_empty_think(c);
        self.push(
            c,
            Action::CompleteTask {
                work: work.id,
                param,
            },
        );
        self.push(
            c,
            Action::PlayAnimation {
                anim: 0,
                duration: ticks(0.25),
                elapsed: 0,
            },
        );
    }

    /// The seeding view of the town's map, for planting.
    pub(crate) fn seeder(&mut self) -> Seeder<'_> {
        self.removed.clear();
        Seeder {
            templates: self.templates,
            terrain: self.terrain,
            cells: self.cells,
            rng: self.rng,
            placed: self.placed,
            removed: &mut self.removed,
            animation_events: &mut self.animation_events,
        }
    }

    /// CreateNaturalResource's start once the cell was found free.
    pub fn plant(&mut self, template: usize, cell: i32) {
        self.seeder().plant(template, cell);
        *self.models_dirty = true;
    }
}
