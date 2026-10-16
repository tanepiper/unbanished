//! Markets: MarketComponent (a ResourceProduceComponent, vtable `0x14015fd60`, work from slot
//! `+0x90` `FUN_14004f3a0`), its work MarketWork (vtable `0x140167b40`), and
//! MarketLocationComponent (states `FUN_14004fa70`). See docs/MARKETS.md.
//!
//! Vendors keep the market stocked by category, each up to its share of the market's room.
//! A category stocked beyond twice its share is carried away first. Otherwise a vendor picks
//! the town storage most worth the trip (the goods most wanted, by distance), and brings what
//! it can carry of the categories still under twice their share.

use std::collections::BTreeMap;

use crate::jobs::{Work, WorkKind};
use crate::town::{Haul, Sim};

/// MarketWork `+0x48`: each vendor's plan (material, count) on its way to the market; empty
/// for a vendor carrying goods away.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MarketWork {
    pub plans: BTreeMap<usize, Vec<(usize, i32)>>,
}

/// A category of the try's table (0x20 bytes): its flags, the units the market holds, the
/// units on their way, the units in the town's storages, the fill, the fill over its share,
/// the units in the storage weighed, and the room left.
#[derive(Debug, Clone, Copy, Default)]
struct Category {
    flags: u32,
    have: i32,
    inbound: i32,
    town: i32,
    fill: f32,
    ratio: f32,
    here: i32,
    room: i32,
}

/// The try's per-material tally (`+8` in the market, `+0xc` in the town, `+0x10` on its way).
#[derive(Debug, Clone, Copy, Default)]
struct Tally {
    have: i32,
    town: i32,
    inbound: i32,
}

impl Sim<'_, '_> {
    /// Slot `+0x90` (`FUN_14004f3a0`).
    pub(crate) fn market_work(&self, e: usize) -> Work {
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
            WorkKind::Market(MarketWork::default()),
        )
    }

    /// A building that produces (ConsumeProduceComponent): vendors pass its storage over.
    fn produces(&self, e: usize) -> bool {
        self.sites
            .get(&e)
            .is_some_and(|s| self.site_templates[s.template].produce.is_some())
    }

    /// A random use point of the market (`Rand` over its InteractComponent's points).
    fn market_point(&mut self, e: usize) -> [f32; 3] {
        let count = self.templates[self.placed[e].template].use_points.len();
        let k = self.rng.rand_below(count as u32) as usize;
        self.use_point(e, k).0
    }

    /// MarketWork's slot `+0x20` (`FUN_1400f5450`).
    pub(crate) fn market_try(&mut self, work: &mut Work, c: usize) -> u8 {
        let e = work.entity;
        let WorkKind::Market(mw) = &work.kind else {
            return 0;
        };
        if self.work_max(e) <= mw.plans.len() as i32 {
            return 1;
        }
        if 0 < self.states[c].carried.used(self.materials) {
            self.drop_off(c);
            return 2;
        }
        let Some(d) = self
            .sites
            .get(&e)
            .and_then(|s| self.site_templates[s.template].market.clone())
        else {
            return 0;
        };
        let storage = self.sites[&e].storage;
        let here = self.citizen_cell(c);
        // `FUN_14011ba10`: the room the vendor has.
        let carried = &self.states[c].carried;
        let mut carry = carried.capacity - carried.used(self.materials);
        let cap = self.storages[storage].capacity as f32;
        if d.threshold <= self.storages[storage].used(self.materials) as f32 / cap {
            return 1;
        }
        let mut cats: Vec<Category> = d
            .quotas
            .iter()
            .map(|&(flags, _)| Category {
                flags,
                ..Default::default()
            })
            .collect();
        let mut tally: BTreeMap<usize, Tally> = BTreeMap::new();
        let flags_of = |sim: &Self, m: usize| sim.materials[m].flags;
        // What the market holds (the per-material tally adds `size × count²`, as the original).
        for item in self.storages[storage].items.clone() {
            let avail = item.count - item.reserved;
            let units = self.materials[item.material].size * avail;
            tally.entry(item.material).or_default().have += avail * units;
            let flags = flags_of(self, item.material);
            for cat in cats.iter_mut().filter(|k| k.flags & flags != 0) {
                cat.have += units;
            }
        }
        // What is on its way.
        if let WorkKind::Market(mw) = &work.kind {
            for plan in mw.plans.values() {
                for &(m, n) in plan {
                    tally.entry(m).or_default().inbound += n;
                    let units = self.materials[m].size * n;
                    let flags = flags_of(self, m);
                    for cat in cats.iter_mut().filter(|k| k.flags & flags != 0) {
                        cat.inbound += units;
                    }
                }
            }
        }
        // What the town's storages hold (not the workshops').
        for site in self.storage_sites.iter().filter(|s| s.radius.is_none()) {
            if self.produces(site.entity) {
                continue;
            }
            for item in &self.storages[site.storage].items {
                let units = (item.count - item.reserved) * self.materials[item.material].size;
                tally.entry(item.material).or_default().town += units;
                let flags = self.materials[item.material].flags;
                for cat in cats.iter_mut().filter(|k| k.flags & flags != 0) {
                    cat.town += units;
                }
            }
        }
        let inv = 1.0 / cap;
        for (cat, &(_, share)) in cats.iter_mut().zip(&d.quotas) {
            cat.fill = (cat.inbound + cat.have) as f32 * inv;
            cat.ratio = cat.fill / share;
            cat.room = (cap * share) as i32 - cat.inbound - cat.have;
        }
        // A category with more than twice its share: the goods above 1.25 times it go to
        // another storage.
        for (i, cat) in cats.iter().enumerate() {
            let share = d.quotas[i].1;
            if share * 2.0 >= cat.have as f32 * inv {
                continue;
            }
            let (have, flags) = (cat.have, cat.flags);
            let keep = (share * 1.25 * cap) as i32;
            for item in self.storages[storage].items.clone() {
                let m = item.material;
                if self.materials[m].flags & flags == 0 {
                    continue;
                }
                let size = self.materials[m].size.max(1);
                let excess = (have - keep) / size;
                let mut amount = carry / size;
                if item.count - item.reserved < amount {
                    amount = item.count - item.reserved;
                }
                if excess < amount {
                    amount = excess;
                }
                if amount <= 0 {
                    continue;
                }
                let Some(found) = self.find_storage_excluding(m, amount, true, here, e) else {
                    continue;
                };
                let source = self.market_point(e);
                let target_entity = self.storage_sites[found.site].entity;
                self.haul(
                    c,
                    work,
                    Haul {
                        target_entity,
                        target_storage: found.storage,
                        target: found.target,
                        source_storage: storage,
                        source_entity: e,
                        source,
                        material: m,
                        amount,
                        param: 0,
                        partial: true,
                        complete_on_pickup: false,
                        tool: true,
                        items: Vec::new(),
                    },
                );
                self.took_job(c, work);
                if let WorkKind::Market(mw) = &mut work.kind {
                    mw.plans.insert(c, Vec::new());
                }
                return 2;
            }
        }
        // The least stocked categories first (`FUN_14002bfe0`).
        crate::ai::crt_qsort_by(&mut cats, |a, b| {
            if a.ratio > b.ratio {
                1
            } else if a.ratio < b.ratio {
                -1
            } else {
                0
            }
        });
        // The storage most worth the trip: the goods each category lacks, the first counting
        // most, over the path rank (or `n +` distance).
        let mut best = None;
        let mut best_score = 0.0f32;
        let sites: Vec<(usize, usize, usize)> = self
            .storage_sites
            .iter()
            .enumerate()
            .filter(|(_, s)| s.radius.is_none())
            .map(|(k, s)| (k, s.entity, s.storage))
            .collect();
        for (k, entity, s) in sites {
            let entrance = self.entrance_cell(entity);
            if self.produces(entity) || entrance < 0 || !self.reachable(here, entrance) {
                continue;
            }
            for cat in cats.iter_mut() {
                cat.here = 0;
            }
            for item in &self.storages[s].items {
                let units = (item.count - item.reserved) * self.materials[item.material].size;
                let flags = self.materials[item.material].flags;
                for cat in cats.iter_mut().filter(|c| c.flags & flags != 0) {
                    cat.here += units;
                }
            }
            let rank = self.rank_or_distance(entrance, here);
            let w = if 0 < rank { 1.0 / rank as f32 } else { 1.0 };
            let mut score = 0.0f32;
            for (i, cat) in cats.iter().enumerate() {
                score += (1.0 - cat.ratio) * cat.here.min(carry) as f32 * (1.0 / (i as f32 + 1.0));
            }
            if best_score < score * w {
                best = Some(k);
                best_score = score * w;
            }
        }
        let Some(k) = best else { return 1 };
        let (src_entity, src_storage) =
            (self.storage_sites[k].entity, self.storage_sites[k].storage);
        // Its goods of a category still under twice its share (the share read at the sorted
        // category's index, as the original): the market's least stocked first.
        let mut cands: Vec<(usize, i32, i32, usize)> = Vec::new();
        for item in self.storages[src_storage].items.clone() {
            let flags = self.materials[item.material].flags;
            if let Some(i) = cats
                .iter()
                .enumerate()
                .position(|(i, cat)| cat.flags & flags != 0 && cat.fill < d.quotas[i].1 * 2.0)
            {
                let t = tally.get(&item.material).copied().unwrap_or_default();
                cands.push((
                    item.material,
                    t.inbound + t.have,
                    item.count - item.reserved,
                    i,
                ));
            }
        }
        crate::ai::crt_qsort_by(&mut cands, |a, b| a.1 - b.1);
        let target = self.market_point(e);
        // The storage's drop point nearest the vendor (`FUN_140132aa0`).
        let source = self
            .storage_point(src_entity, here)
            .unwrap_or_else(|| self.ground_at(self.rect_cell(src_entity)));
        let mut plan = Vec::new();
        for (m, _, avail, i) in cands {
            let size = self.materials[m].size.max(1);
            let mut amount = carry / size;
            if avail < amount {
                amount = avail;
            }
            if cats[i].room / size < amount {
                amount = cats[i].room / size;
            }
            if 0 < amount {
                plan.push((m, amount));
                carry -= amount * size;
                cats[i].room -= amount * size;
            }
        }
        if plan.is_empty() {
            return 1;
        }
        self.haul(
            c,
            work,
            Haul {
                target_entity: e,
                target_storage: storage,
                target,
                source_storage: src_storage,
                source_entity: src_entity,
                source,
                material: usize::MAX,
                amount: -1,
                param: 0,
                partial: true,
                complete_on_pickup: false,
                tool: true,
                items: plan.clone(),
            },
        );
        self.took_job(c, work);
        if let WorkKind::Market(mw) = &mut work.kind {
            mw.plans.insert(c, plan);
        }
        2
    }
}
