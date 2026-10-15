//! FishingComponent (vtable `0x140169660`, the fisherman's dock) and `FishingWork` (vtable
//! `0x140167740`, constructor `FUN_1400eff90`). See docs/JOBS.md "Fishermen".
//!
//! A registered dock counts on map layer 8 within its radius, so docks whose waters overlap
//! share the fish. A worker takes a free use point and a free create point, draws up to ten
//! cells within the radius for water of the description's kinds, and fishes from the use
//! point for 30 s. The catch is the fish's yield times the dock's water share
//! (`1 / docks there · water fraction`), left on the create point.

use crate::character::{Action, MoveTo};
use crate::jobs::{Work, WorkKind};
use crate::pathfind::WALKABLE;
use crate::town::Sim;

/// A resolved FishingDescription (`FUN_140124640`, versioned).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FishingTemplate {
    /// `+0x20`: the catch's raw-material template (a pile template).
    pub output: usize,
    /// `+0x2c`: cell types counted as fishing water.
    pub mask: u32,
    /// `+0x28` (version 1): the stock-manager slot whose limit stops the work.
    pub group: i32,
}

/// FishingComponent `+0x50` registered on the map, `+0x54` the water fraction.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Fishing {
    pub registered: bool,
    pub water: f32,
}

/// `FishingWork`: `+0x48` the worker at each use point, `+0x58` each create point's state (0
/// free, 1 a catch is coming, 2 holds a catch), `+0x68` misses in a row.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FishingWork {
    pub workers: Vec<Option<usize>>,
    pub points: Vec<i32>,
    pub misses: i32,
}

/// The fishing layer of [`crate::seeding::Cells::layers`].
pub const LAYER: usize = 8;

impl Sim<'_, '_> {
    fn fishing_template(&self, e: usize) -> Option<(FishingTemplate, i32)> {
        let t = &self.site_templates[self.sites.get(&e)?.template];
        Some((t.fishing.clone()?, t.radius.unwrap_or(0)))
    }

    /// The map rectangle's centre cell `(x + w / 2, y + h / 2)`.
    fn rect_centre(&self, e: usize) -> (i32, i32) {
        let (x, y) = self.placed[e].cell;
        let (w, h) = self.placed[e].footprint;
        ((w >> 1) + x, (h >> 1) + y)
    }

    /// Slot `+0x90` (`FUN_1401244b0`) → `FUN_1400eff90`.
    pub(crate) fn fishing_work(&self, e: usize) -> Work {
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
            WorkKind::Fishing(FishingWork {
                workers: vec![None; t.use_points.len()],
                points: vec![0; t.create_points.len()],
                misses: 0,
            }),
        )
    }

    /// Slot `+0x98` (`FUN_140124350`, after state 0xd's enter): count on layer 8
    /// (`FUN_1401243a0(1)`), then the water fraction.
    pub(crate) fn fishing_register(&mut self, e: usize) {
        let Some(f) = self.sites.get(&e).and_then(|s| s.fishing) else {
            return;
        };
        if f.registered {
            return;
        }
        self.fishing_layer(e, 1);
        self.fishing_water(e);
        if let Some(f) = self.sites.get_mut(&e).and_then(|s| s.fishing.as_mut()) {
            f.registered = true;
        }
    }

    /// Slot `+0xa0` (`FUN_140124320`: state 0xe's enter and the 0xd → 3 transition).
    pub(crate) fn fishing_unregister(&mut self, e: usize) {
        if !self
            .sites
            .get(&e)
            .and_then(|s| s.fishing)
            .is_some_and(|f| f.registered)
        {
            return;
        }
        self.fishing_layer(e, -1);
        if let Some(f) = self.sites.get_mut(&e).and_then(|s| s.fishing.as_mut()) {
            f.registered = false;
        }
    }

    /// `FUN_1401243a0`.
    fn fishing_layer(&mut self, e: usize, delta: i32) {
        let Some((_, radius)) = self.fishing_template(e) else {
            return;
        };
        let (cx, cy) = self.rect_centre(e);
        self.cells.add_layer(LAYER, cx, cy, radius, delta);
    }

    /// Slot `+0xa8` (`FUN_1401240e0`, also state 0xd's scheduled update): the fishing water
    /// in the disc over half the disc's cells on the map.
    pub(crate) fn fishing_water(&mut self, e: usize) {
        let Some((d, r)) = self.fishing_template(e) else {
            return;
        };
        let (cx, cy) = self.rect_centre(e);
        let n = self.size as i32;
        let (mut cells, mut water) = (0, 0);
        for dx in -r..r {
            for dy in -r..r {
                let (x, y) = (cx + dx, cy + dy);
                if dx * dx + dy * dy > r * r || x < 0 || y < 0 || x >= n || y >= n {
                    continue;
                }
                cells += 1;
                let ty = self.cells.types[(n * y + x) as usize].0;
                if d.mask >> (ty & 0x1f) & 1 != 0 {
                    water += 1;
                }
            }
        }
        if let Some(f) = self.sites.get_mut(&e).and_then(|s| s.fishing.as_mut()) {
            f.water = water as f32 / (cells as f32 * 0.5);
        }
    }

    /// Slot `+0x20` (`FUN_1400ef3f0`).
    pub(crate) fn fishing_try(&mut self, work: &mut Work, c: usize) -> u8 {
        let e = work.entity;
        let Some((d, radius)) = self.fishing_template(e) else {
            return 0;
        };
        let water = self.sites[&e].fishing.unwrap_or_default().water;
        let (count, limit) = self.stock[d.group.clamp(0, 20) as usize];
        self.set_icon(e, crate::professions::icon::LIMIT, limit <= count);
        if limit <= count {
            return 1;
        }
        let WorkKind::Fishing(fw) = &mut work.kind else {
            return 0;
        };
        let Some(slot) = fw.workers.iter().position(Option::is_none) else {
            return 1;
        };
        // Create points: a held catch that has gone is free again; take the first free one.
        let mut free = None;
        for i in 0..fw.points.len() {
            let cell = self.create_cell(e, i);
            if fw.points[i] == 2 && self.cells.objects[cell as usize].is_none() {
                fw.points[i] = 0;
            }
            if free.is_none() && fw.points[i] == 0 {
                free = Some((i, cell));
            }
        }
        let Some((point, out_cell)) = free else {
            // `Jobs::OfferOne(.., 1)` for the catches on the create points, in order.
            let n = fw.points.len();
            self.offers = (0..n)
                .filter_map(|i| self.cells.objects[self.create_cell(e, i) as usize])
                .filter_map(|o| self.piles.get(&o).map(|p| p.work))
                .collect();
            if self.offers.is_empty() {
                return 1;
            }
            self.offers_again = true;
            return crate::jobs::NESTED;
        };
        let (pos, yaw) = self.use_point(e, slot);
        if !self.reachable(self.citizen_cell(c), self.cell_of(pos)) {
            return 1;
        }
        let n = self.size as i32;
        let r2 = radius * radius;
        let (cx, cy) = (work.cell % n, work.cell / n);
        for _ in 0..10 {
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
            // Deviation: a column off the map's edge reads outside the cell array in the
            // original; no catch here.
            if cell < 0 || cell >= n * n {
                continue;
            }
            let i = cell as usize;
            let docks = self.cells.layers[i][LAYER];
            if d.mask >> (self.cells.types[i].0 & 0x1f) & 1 == 0 || docks == 0 {
                continue;
            }
            let factor = 1.0 / docks as f32 * water;
            let param = ((point as i32 | 0x800) << 4) | slot as i32;
            self.fishing_sequence(c, work, pos, yaw, d.output, factor, out_cell, param);
            self.took_job(c, work);
            if let WorkKind::Fishing(fw) = &mut work.kind {
                fw.points[point] = 1;
                fw.workers[slot] = Some(c);
                fw.misses = 0;
            }
            return 2;
        }
        let misses = match &work.kind {
            WorkKind::Fishing(fw) => fw.misses,
            _ => 0,
        };
        if misses < 0xb {
            self.wait_sequence(c, work, slot as i32);
            self.took_job(c, work);
            if let WorkKind::Fishing(fw) = &mut work.kind {
                fw.workers[slot] = Some(c);
                fw.misses += 1;
            }
            return 2;
        }
        if self.states[c].carried.used(self.materials) < 1 {
            return 1;
        }
        self.drop_off(c);
        2
    }

    /// `FUN_1400c3fb0`: take the rod, walk to the use point, fish 30 s facing out, leave the
    /// catch on the create point, report, put the rod away. (Its message to the
    /// FishingComponent reaches the default handler, which does nothing; not ported.)
    #[allow(clippy::too_many_arguments)]
    fn fishing_sequence(
        &mut self,
        c: usize,
        work: &mut Work,
        pos: [f32; 3],
        yaw: f32,
        output: usize,
        factor: f32,
        out_cell: i32,
        param: i32,
    ) {
        self.clear_citizen(c, Some(work));
        self.on_empty_think(c);
        self.push(c, Action::ShowTool(-1));
        self.push(
            c,
            Action::CompleteTask {
                work: work.id,
                param,
            },
        );
        let batch = self.states[c].carried.capacity;
        self.push(
            c,
            Action::CreateRawMaterial {
                template: output,
                cell: out_cell,
                batch,
                workplace: Some(work.entity),
                factor,
            },
        );
        self.push(c, self.perform(c, yaw, 0xe, 30.0, true));
        let walk = MoveTo::new(pos, 2, 0, WALKABLE, 0, -1.0, self.rng);
        self.push(c, Action::move_to(walk));
        self.push(c, Action::ShowTool(5));
    }

    /// Slot `+0x30` (`FUN_1400ef380`; the fail slot `FUN_1400ef350` calls it): the use point
    /// is free; with bit 15 the create point holds a catch.
    pub(crate) fn fishing_complete(work: &mut Work, param: i32) {
        let WorkKind::Fishing(fw) = &mut work.kind else {
            return;
        };
        if let Some(w) = fw.workers.get_mut((param & 0xf) as usize) {
            *w = None;
        }
        if param >> 15 & 1 != 0 {
            if let Some(p) = fw.points.get_mut(((param >> 4) & 0xf) as usize) {
                *p = 2;
            }
        }
    }

    /// Slot `+0x48` (`FUN_1400ef100`): workers that lost the task free their use point;
    /// then create points under way that no remaining worker holds hold a catch.
    pub(crate) fn fishing_refresh(&mut self, work: &mut Work) {
        let id = work.id;
        let WorkKind::Fishing(fw) = &mut work.kind else {
            return;
        };
        let mut lost = false;
        let mut held = Vec::new();
        for w in fw.workers.iter_mut() {
            let Some(c) = *w else { continue };
            match self.characters[c].stack.holds_task(id) {
                None => {
                    *w = None;
                    lost = true;
                }
                Some(param) => held.push((param >> 4) & 0xf),
            }
        }
        if lost {
            for (i, p) in fw.points.iter_mut().enumerate() {
                if !held.contains(&(i as i32)) && *p == 1 {
                    *p = 2;
                }
            }
        }
    }
}
