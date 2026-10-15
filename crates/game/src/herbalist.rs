//! HerbalistComponent (vtable `0x14015f980`, the gatherer's shelter and the herbalist) and its
//! work (vtable `0x1401679c0`, made by slot `+0x90` `FUN_14004bac0`), with the workplace's
//! AutoPickupComponent. See docs/JOBS.md "Gatherers".
//!
//! A worker first takes one of the piles its own building made (AutoPickup). Otherwise, while
//! the stock is under the production group's limit, it draws random cells within the
//! building's radius until one holds an unclaimed, fully grown natural resource of a listed
//! kind, and harvests it. The pile it leaves is recorded for the AutoPickup. Too many
//! unlucky draws in a row and the worker drops off what it carries instead of waiting.

use std::collections::BTreeSet;

use crate::jobs::{Work, WorkKind};
use crate::pathfind::nearest_neighbour;
use crate::town::{cell_ground, Sim};

/// The work's state: `+0x48` the workers (a set; `+0x50` its size), `+0x58` misses in a row.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HerbalistWork {
    pub workers: BTreeSet<usize>,
    pub misses: i32,
}

/// AutoPickupComponent (`+0x28` on, from AutoPickupDescription `+0x20`; `+0x30` the Pickup
/// works of the piles the workplace's workers made).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AutoPickup {
    pub enabled: bool,
    pub works: Vec<i32>,
}

/// A resolved HerbalistDescription.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HerbalistTemplate {
    /// `+0x20`: the natural-resource templates (indices into the placed-object templates).
    pub resources: Vec<usize>,
    /// `+0x30`: the stock-manager slot whose limit stops the work.
    pub group: i32,
}

impl Sim<'_, '_> {
    /// Slot `+0x90` (`FUN_14004bac0`).
    pub(crate) fn herbalist_work(&self, e: usize) -> Work {
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
            WorkKind::Herbalist(HerbalistWork::default()),
        )
    }

    /// `FUN_1400e9280`: queue the AutoPickup's works that still exist, in order, to be offered
    /// (`Jobs::OfferOne` with a second try); the others leave the list. False when there is
    /// nothing to offer.
    pub(crate) fn auto_pickup_offers(&mut self, e: usize, exists: impl Fn(i32) -> bool) -> bool {
        let Some(a) = self.sites.get_mut(&e).and_then(|s| s.auto_pickup.as_mut()) else {
            return false;
        };
        if !a.enabled {
            return false;
        }
        a.works.retain(|&id| exists(id));
        if a.works.is_empty() {
            return false;
        }
        self.offers = a.works.clone();
        self.offers_again = true;
        true
    }

    /// `FUN_140045170`: a pile's Pickup work joins its workplace's AutoPickup list.
    pub(crate) fn auto_pickup_add(&mut self, e: usize, work: i32) {
        if let Some(a) = self.sites.get_mut(&e).and_then(|s| s.auto_pickup.as_mut()) {
            if a.enabled {
                a.works.push(work);
            }
        }
    }

    /// Slot `+0x20` (`FUN_1400f3880`) after the AutoPickup offers.
    pub(crate) fn herbalist_try(&mut self, work: &mut Work, c: usize) -> u8 {
        let e = work.entity;
        let Some(site) = self.sites.get(&e) else {
            return 0;
        };
        let t = &self.site_templates[site.template];
        let Some(desc) = t.herbalist.clone() else {
            return 0;
        };
        let radius = t.radius.unwrap_or(0);
        // The production group's limit (also the status icon 2).
        let (count, limit) = self.stock[desc.group.clamp(0, 20) as usize];
        let limited = limit <= count;
        self.set_icon(e, crate::professions::icon::LIMIT, limited);
        let WorkKind::Herbalist(h) = &work.kind else {
            return 0;
        };
        if self.work_max(e) < h.workers.len() as i32 || limited {
            return 1;
        }
        let n = self.size as i32;
        let r2 = radius * radius;
        let attempts = (r2 as f32 * std::f32::consts::PI).sqrt() as i32;
        let (cx, cy) = (work.cell % n, work.cell / n);
        let from = self.citizen_cell(c);
        let entrance = self.entrance_cell(e);
        let mut hit = None;
        'draws: for _ in 0..attempts {
            let (x, y) = loop {
                let (dx, dy) = loop {
                    let dx = self.rng.rand_tol(radius);
                    let dy = self.rng.rand_tol(radius);
                    if dx * dx + dy * dy <= r2 {
                        break (dx, dy);
                    }
                };
                // The original's column test (`x < 0 && n <= x`) can never hold: only the
                // row is kept on the map.
                let (x, y) = (cx + dx, cy + dy);
                if 0 <= y && y < n {
                    break (x, y);
                }
            };
            let cell = n * y + x;
            // Deviation: a column off the map's edge reads past the cell array in the
            // original; such a cell is no candidate here.
            if cell < 0 || cell >= n * n {
                continue;
            }
            let ty = self.cells.types[cell as usize].0;
            if (1u32 << (ty & 0x1f)) & 0x1e0 != 0 {
                continue;
            }
            if self.geometry_rank(entrance, cell) > radius * 2 {
                continue;
            }
            let Some(o) = self.cells.objects[cell as usize] else {
                continue;
            };
            let p = &self.placed[o];
            if self.templates[p.template].natural.is_none() {
                continue;
            }
            for &kind in &desc.resources {
                if p.growth < 1.0 || p.template != kind || self.claimed.contains(&o) {
                    continue;
                }
                let Some(nb) = nearest_neighbour(self.size, &self.cells.types, cell, from, 0xe07)
                else {
                    continue;
                };
                if !self.reachable(nb, from) {
                    continue;
                }
                hit = Some((o, nb));
                break 'draws;
            }
        }
        if let Some((o, nb)) = hit {
            let point = cell_ground(self.size, self.terrain, nb);
            self.removal_sequence(c, work, o, point, true);
            self.took_job(c, work);
            if let WorkKind::Herbalist(h) = &mut work.kind {
                h.misses = 0;
                h.workers.insert(c);
            }
            return 2;
        }
        let misses = match &work.kind {
            WorkKind::Herbalist(h) => h.misses,
            _ => 0,
        };
        if attempts < misses {
            // `FUN_1401323e0` of the carried storage, then `Activity::DropOff`.
            if self.states[c].carried.used(self.materials) < 1 {
                return 0;
            }
            self.drop_off(c);
            return 2;
        }
        self.wait_sequence(c, work, 0);
        self.took_job(c, work);
        if let WorkKind::Herbalist(h) = &mut work.kind {
            h.workers.insert(c);
            h.misses += 1;
        }
        2
    }

    /// `FUN_14008d480(entity, 0)` on the town view.
    pub(crate) fn entrance_cell(&mut self, e: usize) -> i32 {
        let town = crate::activities::TownView {
            size: self.size,
            types: &self.cells.types,
            regions: self.regions,
            occupants: &self.cells.occupants,
            objects: &self.cells.objects,
            entities: self.entities,
            happiness: self.happiness,
        };
        town.entrance(e, self.rng)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn attempts_follow_the_disc_area() {
        // `(int)sqrt(r² · π)` for the gatherer's radius of 20 cells.
        let r2 = 20 * 20;
        assert_eq!((r2 as f32 * std::f32::consts::PI).sqrt() as i32, 35);
    }
}
