//! HuntComponent (vtable `0x140169ba0`, the hunter's lodge) and its work (vtable
//! `0x140167a40`, made by slot `+0x90` `FUN_140127b70`). See docs/JOBS.md "Hunters".
//!
//! The lodge gathers hunting time while it is staffed (`FUN_140127c10`). A hunter walks to a
//! random empty cell in the radius, or waits there, with a `HuntAnimal` watching. Once the
//! lodge has gathered enough time, a hunter who comes within 8 of a wild animal goes to it and
//! takes it (`FUN_1400cd2b0` → `FUN_1400c1370`): its meat and hide are left where it fell.

use crate::character::{Action, MoveTo};
use crate::herbalist::HerbalistWork;
use crate::jobs::{Work, WorkKind};
use crate::pathfind::WALKABLE;
use crate::scheduler::{seconds, ticks};
use crate::town::{cell_ground, Sim};

impl Sim<'_, '_> {
    /// Slot `+0x90` (`FUN_140127b70`).
    pub(crate) fn hunt_work(&self, e: usize) -> Work {
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
            WorkKind::Hunt(HerbalistWork::default()),
        )
    }

    /// HuntComponent's scheduled update (`FUN_140127c10`): the hunting time grows by the
    /// elapsed time times the workers over the most allowed.
    pub fn update_hunts(&mut self, dt: i64) {
        let lodges: Vec<usize> = self
            .sites
            .iter()
            .filter(|(_, s)| s.hunt_time.is_some())
            .map(|(&e, _)| e)
            .collect();
        for e in lodges {
            let workers = self
                .workforce
                .workplaces
                .get(&e)
                .map_or(0, |w| w.workers.len()) as f32;
            let max = self.work_max(e) as f32;
            // Deviation: no workers allowed would divide by zero in the original.
            if max <= 0.0 {
                continue;
            }
            let add = ticks(seconds(dt) * (workers / max));
            if let Some(t) = self.sites.get_mut(&e).and_then(|s| s.hunt_time.as_mut()) {
                *t += add;
            }
        }
    }

    /// Slot `+0x20` (`FUN_1400f44c0`) after the AutoPickup offers.
    pub(crate) fn hunt_try(&mut self, work: &mut Work, c: usize) -> u8 {
        let e = work.entity;
        if self.states[c].carried.used(self.materials) >= 1 {
            self.drop_off(c);
            return 2;
        }
        let Some(site) = self.sites.get(&e) else {
            return 0;
        };
        let t = &self.site_templates[site.template];
        let Some(d) = t.hunt else { return 0 };
        let radius = t.radius.unwrap_or(0);
        let (count, limit) = self.stock[d.group.clamp(0, 20) as usize];
        // Not ported: building status icons (`FUN_1401318c0(icon, 2, limited)`).
        if limit <= count {
            return 1;
        }
        let WorkKind::Hunt(h) = &work.kind else {
            return 0;
        };
        if self.work_max(e) < h.workers.len() as i32 {
            return 1;
        }
        let n = self.size as i32;
        let (cx, cy) = (work.cell % n, work.cell / n);
        let (x, y) = loop {
            let (dx, dy) = loop {
                let dx = self.rng.rand_tol(radius);
                let dy = self.rng.rand_tol(radius);
                if dx * dx + dy * dy <= radius * radius {
                    break (dx, dy);
                }
            };
            // As in the gatherer's, only the row is kept on the map.
            let (x, y) = (cx + dx, cy + dy);
            if 0 <= y && y < n {
                break (x, y);
            }
        };
        let from = self.citizen_cell(c);
        let entrance = self.entrance_cell(e);
        let cell = n * y + x;
        // Deviation: a column off the map's edge reads outside the cell array; no walk.
        let walk = 0 <= cell
            && cell < n * n
            && entrance >= 0
            && (1u32 << (self.cells.types[cell as usize].0 & 0x1f)) & 0x1e0 == 0
            && self.geometry_rank(entrance, cell) <= radius * 2
            && self.cells.objects[cell as usize].is_none()
            && self.reachable(cell, from);
        let point = walk.then(|| cell_ground(self.size, self.terrain, cell));
        self.hunt_sequence(c, work, e, point);
        self.took_job(c, work);
        if let WorkKind::Hunt(h) = &mut work.kind {
            h.workers.insert(c);
        }
        2
    }

    /// `FUN_1400c1090`: walk to `point` (or work 5 s where it stands), watched by a
    /// `HuntAnimal`, then report.
    fn hunt_sequence(&mut self, c: usize, work: &mut Work, lodge: usize, point: Option<[f32; 3]>) {
        self.clear_citizen(c, Some(work));
        self.on_empty_think(c);
        self.push(
            c,
            Action::CompleteTask {
                work: work.id,
                param: 0,
            },
        );
        let action = match point {
            Some(p) => Action::move_to(MoveTo::new(p, 1, 0, WALKABLE, 0, -1.0, self.rng)),
            None => self.perform(c, -1000.0, 8, 5.0, true),
        };
        self.push(c, action);
        self.characters[c]
            .stack
            .attach(Action::HuntAnimal { lodge, done: false });
    }
}
