//! The job manager (`Game +0x118`, constructor `FUN_1400fe650`). See docs/JOBS.md.
//!
//! Every `Work` gets a sequential id (`+0x00`, from 1) and a priority (`+0x04`, sequential
//! unless the owner keeps one). The manager indexes works by id (`+0x30`), by priority
//! (`+0x10`), by category (`+0x40 + (c − 1)·0x10`, categories 1..14 from the registration
//! mask's bits) and by profession (`+0x120`, the work's slot `+0x50`). All of them are trees
//! keyed by id, so iteration is in id order.
//!
//! The AI's work (9) and emergency (2) activities ask the manager for a job:
//!
//! - [`Jobs::assign_work`] (`FUN_1400fcd00`): the citizen's own profession's works, then
//!   [`Jobs::assign_emergency`], then the laborers' works;
//! - [`Jobs::assign_emergency`] (`FUN_1400fca20`): category lists chosen from low stock
//!   (`Rand(n)` on the game stream), each tried by distance;
//! - `try_list` (`FUN_1400fce80`) sorts candidate works by priority (`FUN_1400fd4d0`) or path
//!   rank (`FUN_1400fd160`), ties by id, and `offer` (`FUN_1400fd030`) offers them in order.
//!
//! A work's virtual slots are supplied by a [`WorkHost`]: `+0x20` try to assign (0 no,
//! 1 not now, 2 taken), `+0x28` a second try, `+0x30` a task completed, `+0x38` a task failed,
//! `+0x40` cancel every worker, `+0x48` refresh.

use std::collections::{BTreeMap, BTreeSet};

use crate::random::Random;

/// Categories 1..14 (the registration mask's bits 1..14).
pub const CATEGORIES: usize = 14;
/// Construction (`BuildStructure` and `GatherRawMaterials`, mask `0x2001`).
pub const CONSTRUCTION: u32 = 13;
/// Emergency (fire) work, the only category a citizen with an emergency tries.
pub const EMERGENCY: u32 = 14;

/// The kinds of work the port has, with their own state.
#[derive(Debug, Clone)]
pub enum WorkKind {
    /// `GatherRawMaterials` (vtable `0x140167840`, constructor `FUN_1400f1be0`).
    Gather(crate::construction::Gather),
    /// `BuildStructure` (vtable `0x1401674c0`, constructor `FUN_1400eae90`).
    Build(crate::construction::BuildWork),
    /// `ClearArea` (vtable `0x140167540`, constructor `FUN_1400ebe30`).
    Clear(crate::construction::ClearWork),
    /// `Pickup` (vtable `0x140167c30`, built by RawMaterialComponent activation `FUN_14012e6a0`).
    Pickup(crate::construction::PickupWork),
    /// `ConsumeProduce` (vtable `0x140167640`, constructor `FUN_1400ee6d0`, made by
    /// ConsumeProduceComponent slot `+0x90` `FUN_14011e1d0`).
    Produce(crate::construction::ProduceWork),
    /// `LocationWork` (vtable `0x140167ab0`): staff stand at a school, chapel or doctor.
    Location(crate::services::LocationWork),
    /// HerbalistComponent's work (vtable `0x1401679c0`, made by `FUN_14004bac0`): gatherers
    /// and herbalists.
    Herbalist(crate::herbalist::HerbalistWork),
    /// ForesterComponent's work (vtable `0x1401677c0`, made by `FUN_140124c50`), with the
    /// same fields.
    Forester(crate::herbalist::HerbalistWork),
    /// HuntComponent's work (vtable `0x140167a40`, made by `FUN_140127b70`), with the
    /// gatherer's fields.
    Hunt(crate::herbalist::HerbalistWork),
    /// `PlantArea` (vtable `0x140167cb0`, constructor `FUN_1400f9b70`).
    PlantArea(crate::farm::AreaWork),
    /// `TendArea` (vtable `0x140167db0`): the workers, as the gatherer's.
    TendArea(crate::herbalist::HerbalistWork),
    /// The harvest work (vtable `0x1401678c0`, constructor `FUN_1400f2990`).
    Harvest(crate::farm::AreaWork),
    /// `PlantOrchard` (vtable `0x140167d30`, constructor `FUN_1400fa610`): `need` marks the
    /// slots being planted.
    PlantOrchard(crate::farm::AreaWork),
    /// Fruit picking (vtable `0x140167940`, constructor `FUN_1400f3530`).
    PickFruit(crate::farm::AreaWork),
    /// PastureComponent's work (vtable `0x140167bc0`, constructor `FUN_1400f8250`): the
    /// ranchers tending (`+0x48`), as the gatherer's.
    Pasture(crate::herbalist::HerbalistWork),
    /// `ClearStorage` (vtable `0x1401675c0`, constructor `FUN_1400ec920`): an empty house's
    /// goods go to the town's storage; `+0x58` the haulers.
    ClearStorage(crate::herbalist::HerbalistWork),
    /// MarketWork (vtable `0x140167b40`, made by MarketComponent's slot `+0x90`).
    Market(crate::market::MarketWork),
    /// `FishingWork` (vtable `0x140167740`, constructor `FUN_1400eff90`).
    Fishing(crate::fishing::FishingWork),
    /// A test stand-in.
    Test(i32),
}

/// `Work` (vtable `0x140167e30`, constructor `FUN_1400fc7b0`).
#[derive(Debug, Clone)]
pub struct Work {
    /// `+0x10`: the owning entity.
    pub entity: usize,
    /// `+0x20` id and `+0x24` priority.
    pub id: i32,
    pub priority: i32,
    /// `+0x28`: the owner's map-rectangle centre cell.
    pub cell: i32,
    /// `+0x2c`: the manager pass in which it last answered "not now".
    pub failed_pass: i32,
    /// `+0x30`, `+0x38`, `+0x40`: the distance-patience clock of `FUN_1400fc3c0`.
    pub patience: i64,
    pub patience_base: i64,
    pub far_count: i32,
    /// Slot `+0x50`: the profession whose citizens do this work.
    pub profession: usize,
    pub kind: WorkKind,
}

impl Work {
    /// `FUN_1400fc7b0`: `cell` is `(y + h/2)·n + x + w/2` of the owner's map rectangle.
    pub fn new(entity: usize, cell: i32, profession: usize, kind: WorkKind) -> Work {
        Work {
            entity,
            id: -1,
            priority: 0,
            cell,
            failed_pass: 0,
            patience: 0,
            patience_base: 0,
            far_count: 0,
            profession,
            kind,
        }
    }
}

/// The citizen and world queries and the work slots the manager calls.
pub trait WorkHost {
    /// Citizen `+0x1f0`, `+0x248`, the character's cell, and `FUN_14011b0b0`.
    fn profession(&self, citizen: usize) -> usize;
    fn workplace(&self, citizen: usize) -> Option<usize>;
    fn cell(&self, citizen: usize) -> i32;
    fn has_emergency(&self, citizen: usize) -> bool;
    /// Profession `+0x4c`.
    fn works_anywhere(&self, profession: usize) -> bool;
    /// `FUN_1400a4ce0(map, a, b, 1)`, with the callers' fallback `n + (int)sqrtf(d²)`.
    fn rank(&mut self, a: i32, b: i32) -> i32;
    /// The stock manager's `(count, limit)` per slot (`Game +0xf0 → +0x170`).
    fn stock(&self, slot: usize) -> (i32, i32);
    /// The laborer and builder professions (`Game +0xb0 → +0x30/+0x38`).
    fn laborer(&self) -> usize;
    fn builder(&self) -> usize;
    fn rng(&mut self) -> &mut Random;

    /// Slot `+0x48`: refresh (drop workers that no longer hold this work's task).
    fn refresh(&mut self, work: &mut Work);
    /// Slot `+0x20`: 0 cannot, 1 not now, 2 assigned. `time` is the manager's clock `+0x130`.
    fn try_assign(&mut self, work: &mut Work, citizen: usize, time: i64) -> u8;
    /// Slot `+0x28` (the base returns false).
    fn try_assign_again(&mut self, _work: &mut Work, _citizen: usize, _time: i64) -> bool {
        false
    }
    /// Slot `+0x30`: a CompleteTask finished; true removes the work.
    fn complete(&mut self, work: &mut Work, citizen: usize, param: i32) -> bool;
    /// Slot `+0x38`: a CompleteTask was aborted.
    fn fail(&mut self, work: &mut Work, citizen: usize, param: i32);
    /// Job messages raised while the host ran a slot (aborted CompleteTasks of other works),
    /// for the manager to deliver: `(work, citizen, param, completed)`.
    fn take_events(&mut self) -> Vec<JobEvent> {
        Vec::new()
    }
    /// Slot `+0x40`: clear every worker's action stack (their CompleteTask aborts call
    /// [`WorkHost::fail`] on this work, which the host handles directly).
    fn cancel(&mut self, work: &mut Work);
    /// Works a slot `+0x20` asked the manager to offer in its place (`FUN_1400fdaf0` called
    /// from inside a try, as ClearArea does with the piles in its footprint), in order. A try
    /// answering [`NESTED`] has queued them.
    fn take_offers(&mut self) -> Vec<i32> {
        Vec::new()
    }
    /// The queued offers get a second try (`FUN_1400fdaf0(.., 1)`).
    fn offers_again(&mut self) -> bool {
        false
    }
    /// After a [`NESTED`] answer none of whose offers was taken: the rest of the try
    /// (ConsumeProduce goes on to work a cycle).
    fn resume(&mut self, _work: &mut Work, _citizen: usize) -> u8 {
        1
    }
}

/// A try's answer meaning "offer these other works for me" ([`WorkHost::take_offers`]).
pub const NESTED: u8 = 3;

/// A CompleteTask's report: `complete` (start) or failed (abort).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JobEvent {
    pub work: i32,
    pub citizen: usize,
    pub param: i32,
    pub complete: bool,
}

/// The manager.
#[derive(Debug, Clone)]
pub struct Jobs {
    /// `+0x00`: last id (wraps from `0x7ffffffe` to 0); `+0x04`: next sequential priority.
    next_id: i32,
    next_priority: i32,
    /// `+0x08`: pass counter (one per simulation iteration, `FUN_14009b850`).
    pub pass: i32,
    /// `+0x10`: priority → id.
    priorities: BTreeMap<i32, i32>,
    /// `+0x30`: id → work.
    pub works: BTreeMap<i32, Work>,
    /// `+0x40..`: category 1..14 → ids.
    categories: [BTreeSet<i32>; CATEGORIES],
    /// `+0x120`: profession → ids.
    professions: BTreeMap<usize, BTreeSet<i32>>,
    /// `+0x130`: manager clock (ticks), advanced with the pass.
    pub time: i64,
}

impl Default for Jobs {
    fn default() -> Self {
        Jobs {
            next_id: 0,
            next_priority: 0,
            pass: 1,
            priorities: BTreeMap::new(),
            works: BTreeMap::new(),
            categories: Default::default(),
            professions: BTreeMap::new(),
            time: 0,
        }
    }
}

/// `FUN_1400fca20`'s stock checks: `(slot, categories)`, tried when `count <= limit / 10`.
const EMERGENCY_LISTS: [(usize, &[u32]); 9] = [
    (5, &[1]),
    (3, &[2, 4, 10]),
    (4, &[3, 6, 4, 10, 12]),
    (0, &[4, 10]),
    (2, &[6, 12]),
    (1, &[5, 11]),
    (10, &[9]),
    (7, &[7, 9]),
    (6, &[8]),
];

impl Jobs {
    /// `FUN_1400fe650` + the per-iteration step in `FUN_14009b850`.
    pub fn step(&mut self, dt: i64) {
        self.pass = self.pass.wrapping_add(1);
        self.time = self.time.wrapping_add(dt);
    }

    pub fn is_empty(&self) -> bool {
        self.works.is_empty()
    }

    /// `FUN_1400fdd70(work, mask, keep, priority)`: register a work. `keep` uses `priority`
    /// (moved down past any taken value) instead of the next sequential one. Returns the id.
    pub fn add(&mut self, mut work: Work, mask: u32, keep: Option<i32>) -> i32 {
        self.next_id = self.next_id.wrapping_add(1);
        let id = self.next_id;
        if id == 0x7ffffffe {
            self.next_id = 0;
        }
        let priority = match keep {
            None => {
                let p = self.next_priority;
                self.next_priority = p.wrapping_add(1);
                p
            }
            Some(mut p) => {
                while self.priorities.contains_key(&p) {
                    p -= 1;
                }
                p
            }
        };
        work.id = id;
        work.priority = priority;
        self.priorities.entry(priority).or_insert(id);
        for c in 1..=CATEGORIES {
            if mask & (1 << c) != 0 {
                self.categories[c - 1].insert(id);
            }
        }
        self.professions
            .entry(work.profession)
            .or_default()
            .insert(id);
        self.works.insert(id, work);
        id
    }

    /// `FUN_1400fe300`: unregister, refresh, cancel every worker, delete.
    pub fn remove(&mut self, id: i32, host: &mut impl WorkHost) {
        let Some(mut work) = self.works.remove(&id) else {
            return;
        };
        for c in &mut self.categories {
            c.remove(&id);
        }
        if self.priorities.get(&work.priority) == Some(&id) {
            self.priorities.remove(&work.priority);
        }
        if let Some(set) = self.professions.get_mut(&work.profession) {
            set.remove(&id);
        }
        host.refresh(&mut work);
        host.cancel(&mut work);
        self.drain(host);
    }

    /// Deliver the host's pending job events in order.
    pub fn drain(&mut self, host: &mut impl WorkHost) {
        loop {
            let events = host.take_events();
            if events.is_empty() {
                return;
            }
            for e in events {
                if e.complete {
                    self.complete(e.work, e.citizen, e.param, host);
                } else {
                    self.fail(e.work, e.citizen, e.param, host);
                }
            }
        }
    }

    /// `FUN_1400fefa0`: move a work to the front (`lowest priority − 1`).
    pub fn prioritize(&mut self, id: i32) {
        let Some(work) = self.works.get(&id) else {
            return;
        };
        let top = self
            .priorities
            .keys()
            .next()
            .map_or(0, |&p| p.wrapping_sub(1));
        let old = work.priority;
        if self.priorities.get(&old) == Some(&id) {
            self.priorities.remove(&old);
        }
        self.works.get_mut(&id).unwrap().priority = top;
        self.priorities.entry(top).or_insert(id);
    }

    /// `FUN_1400fd8d0`: move a work from one profession's list to another's.
    pub fn change_profession(&mut self, id: i32, from: usize, to: usize) {
        if !self.works.contains_key(&id) {
            return;
        }
        let Some(set) = self.professions.get_mut(&from) else {
            self.professions.entry(from).or_default();
            return;
        };
        if !set.remove(&id) {
            return;
        }
        self.professions.entry(to).or_default().insert(id);
        self.works.get_mut(&id).unwrap().profession = to;
    }

    /// `FUN_1400fdc30` (CompleteTask start): a task finished; remove the work when the slot
    /// says it is done.
    pub fn complete(&mut self, id: i32, citizen: usize, param: i32, host: &mut impl WorkHost) {
        let Some(mut work) = self.works.remove(&id) else {
            return;
        };
        let done = host.complete(&mut work, citizen, param);
        self.works.insert(id, work);
        self.drain(host);
        if done {
            self.remove(id, host);
        }
    }

    /// `FUN_1400fdcd0` (CompleteTask abort).
    pub fn fail(&mut self, id: i32, citizen: usize, param: i32, host: &mut impl WorkHost) {
        if let Some(mut work) = self.works.remove(&id) {
            host.fail(&mut work, citizen, param);
            self.works.insert(id, work);
            self.drain(host);
        }
    }

    /// Slot `+0x20` of a work taken out of the table, answering [`NESTED`] by offering the
    /// queued works (`FUN_1400fdaf0` without a second try) until one is taken: then 2, else the
    /// rest of the try (which may offer again). The work is back in the table afterwards.
    fn try_one(&mut self, id: i32, mut work: Work, citizen: usize, host: &mut impl WorkHost) -> u8 {
        let mut r = host.try_assign(&mut work, citizen, self.time);
        self.works.insert(id, work);
        while r == NESTED {
            let again = host.offers_again();
            for other in host.take_offers() {
                if self.offer_one(other, citizen, again, host) {
                    return 2;
                }
            }
            let Some(mut work) = self.works.remove(&id) else {
                return 1;
            };
            r = host.resume(&mut work, citizen);
            self.works.insert(id, work);
        }
        r
    }

    /// `FUN_1400fdaf0`: offer one work by id, with a second try when the first answers
    /// "not now" and `again` is set.
    pub fn offer_one(
        &mut self,
        id: i32,
        citizen: usize,
        again: bool,
        host: &mut impl WorkHost,
    ) -> bool {
        let Some(mut work) = self.works.remove(&id) else {
            return false;
        };
        let mut taken = false;
        if work.failed_pass != self.pass {
            host.refresh(&mut work);
            match self.try_one(id, work, citizen, host) {
                2 => taken = true,
                1 => {
                    if again {
                        if let Some(mut work) = self.works.remove(&id) {
                            host.refresh(&mut work);
                            taken = host.try_assign_again(&mut work, citizen, self.time);
                            self.works.insert(id, work);
                        }
                    }
                    if !taken {
                        if let Some(w) = self.works.get_mut(&id) {
                            w.failed_pass = self.pass;
                        }
                    }
                }
                _ => {}
            }
        } else {
            self.works.insert(id, work);
        }
        self.drain(host);
        taken
    }

    /// `FUN_1400fcd00`.
    pub fn assign_work(&mut self, citizen: usize, host: &mut impl WorkHost) {
        if self.works.is_empty() {
            return;
        }
        let laborer = host.laborer();
        let profession = host.profession(citizen);
        if profession != laborer {
            if let Some(set) = self.professions.get(&profession).cloned() {
                if self.try_list(citizen, &set, profession, false, false, host) {
                    return;
                }
            }
        }
        if !self.assign_emergency(citizen, host) {
            if let Some(set) = self.professions.get(&laborer).cloned() {
                self.try_list(citizen, &set, laborer, false, false, host);
            }
        }
    }

    /// `FUN_1400fca20`.
    pub fn assign_emergency(&mut self, citizen: usize, host: &mut impl WorkHost) -> bool {
        let mut lists: Vec<&[u32]> = Vec::new();
        let emergency = host.has_emergency(citizen);
        if emergency {
            lists.push(&[EMERGENCY]);
        } else {
            if host.profession(citizen) == host.builder() {
                lists.push(&[CONSTRUCTION]);
            }
            for (slot, categories) in EMERGENCY_LISTS {
                let (count, limit) = host.stock(slot);
                if count <= limit / 10 {
                    lists.push(categories);
                }
            }
        }
        let k = host.rng().rand_below(lists.len() as u32) as usize;
        let Some(list) = lists.get(k).copied() else {
            return false;
        };
        let profession = host.profession(citizen);
        for &c in list {
            let set = self.categories[c as usize - 1].clone();
            if self.try_list(citizen, &set, profession, true, emergency, host) {
                return true;
            }
        }
        false
    }

    /// `FUN_1400fce80`: the citizen's workplace's works first (when it has the list's
    /// profession), then, for professions that work anywhere (or in an emergency), all.
    fn try_list(
        &mut self,
        citizen: usize,
        set: &BTreeSet<i32>,
        profession: usize,
        by_distance: bool,
        emergency: bool,
        host: &mut impl WorkHost,
    ) -> bool {
        let workplace = if emergency {
            None
        } else {
            host.workplace(citizen)
        };
        if let Some(wp) = workplace {
            if profession == host.profession(citizen) {
                let order = self.sort(citizen, set, Some(wp), by_distance, host);
                if self.offer(citizen, &order, host) {
                    return true;
                }
            }
        }
        if host.works_anywhere(profession) || emergency {
            let order = self.sort(citizen, set, None, by_distance, host);
            if self.offer(citizen, &order, host) {
                return true;
            }
        }
        false
    }

    /// `FUN_1400fd160` / `FUN_1400fd4d0`: the works of `set` (of entity `owner` when given),
    /// ordered by path rank from the citizen's cell or by priority, then by id.
    fn sort(
        &self,
        citizen: usize,
        set: &BTreeSet<i32>,
        owner: Option<usize>,
        by_distance: bool,
        host: &mut impl WorkHost,
    ) -> Vec<i32> {
        let from = host.cell(citizen);
        let mut keyed: Vec<(i32, i32)> = set
            .iter()
            .filter_map(|id| self.works.get(id))
            .filter(|w| owner.is_none_or(|o| w.entity == o))
            .map(|w| {
                let key = if by_distance {
                    host.rank(from, w.cell)
                } else {
                    w.priority
                };
                (key, w.id)
            })
            .collect();
        keyed.sort();
        keyed.dedup();
        keyed.into_iter().map(|(_, id)| id).collect()
    }

    /// `FUN_1400fd030`.
    fn offer(&mut self, citizen: usize, order: &[i32], host: &mut impl WorkHost) -> bool {
        for &id in order {
            let Some(mut work) = self.works.remove(&id) else {
                continue;
            };
            let mut taken = false;
            if work.failed_pass != self.pass {
                host.refresh(&mut work);
                match self.try_one(id, work, citizen, host) {
                    2 => taken = true,
                    1 => {
                        if let Some(w) = self.works.get_mut(&id) {
                            w.failed_pass = self.pass;
                        }
                    }
                    _ => {}
                }
            } else {
                self.works.insert(id, work);
            }
            self.drain(host);
            if taken {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A host whose works answer from a table and record the order they were offered.
    struct Host {
        rng: Random,
        answers: BTreeMap<i32, u8>,
        offered: Vec<i32>,
        profession: usize,
        workplace: Option<usize>,
        stock: [(i32, i32); 21],
        cells: BTreeMap<i32, i32>,
    }

    impl WorkHost for Host {
        fn profession(&self, _: usize) -> usize {
            self.profession
        }
        fn workplace(&self, _: usize) -> Option<usize> {
            self.workplace
        }
        fn cell(&self, _: usize) -> i32 {
            0
        }
        fn has_emergency(&self, _: usize) -> bool {
            false
        }
        fn works_anywhere(&self, p: usize) -> bool {
            p < 2
        }
        fn rank(&mut self, _: i32, b: i32) -> i32 {
            self.cells.get(&b).copied().unwrap_or(b)
        }
        fn stock(&self, slot: usize) -> (i32, i32) {
            self.stock[slot]
        }
        fn laborer(&self) -> usize {
            0
        }
        fn builder(&self) -> usize {
            1
        }
        fn rng(&mut self) -> &mut Random {
            &mut self.rng
        }
        fn refresh(&mut self, _: &mut Work) {}
        fn try_assign(&mut self, work: &mut Work, _: usize, _: i64) -> u8 {
            self.offered.push(work.id);
            self.answers.get(&work.id).copied().unwrap_or(1)
        }
        fn complete(&mut self, _: &mut Work, _: usize, _: i32) -> bool {
            true
        }
        fn fail(&mut self, _: &mut Work, _: usize, _: i32) {}
        fn cancel(&mut self, _: &mut Work) {}
    }

    fn host() -> Host {
        Host {
            rng: Random::new(7),
            answers: BTreeMap::new(),
            offered: Vec::new(),
            profession: 0,
            workplace: None,
            stock: [(100, 100); 21],
            cells: BTreeMap::new(),
        }
    }

    #[test]
    fn ids_priorities_and_offer_order() {
        let mut jobs = Jobs::default();
        let mut h = host();
        let a = jobs.add(Work::new(5, 10, 0, WorkKind::Test(0)), 1 << 13, None);
        let b = jobs.add(Work::new(6, 20, 0, WorkKind::Test(0)), 1 << 13, Some(0));
        assert_eq!((a, b), (1, 2));
        // The kept priority 0 is taken by `a`, so `b` moves down to -1 and is offered first.
        assert_eq!(jobs.works[&b].priority, -1);
        jobs.assign_work(0, &mut h);
        assert_eq!(h.offered, vec![b, a]);
        // Both answered "not now": skipped for the rest of this pass.
        h.offered.clear();
        jobs.assign_work(0, &mut h);
        assert!(h.offered.is_empty());
        jobs.step(1);
        h.answers.insert(a, 2);
        jobs.assign_work(0, &mut h);
        assert_eq!(h.offered, vec![b, a]);
        jobs.remove(a, &mut h);
        assert_eq!(jobs.works.len(), 1);
    }

    #[test]
    fn builders_try_construction_by_distance() {
        let mut jobs = Jobs::default();
        let mut h = host();
        h.profession = 1;
        let near = jobs.add(Work::new(5, 30, 1, WorkKind::Test(0)), 1 << 13, None);
        let far = jobs.add(Work::new(6, 3, 1, WorkKind::Test(0)), 1 << 13, None);
        h.cells.insert(30, 50);
        h.cells.insert(3, 40);
        // One list (construction): Rand(1) is drawn, then the works by rank 40, 50.
        let mut check = Random::new(7);
        jobs.assign_emergency(0, &mut h);
        check.rand_below(1);
        assert_eq!(h.offered, vec![far, near]);
        assert_eq!(h.rng.rand(), check.rand());
    }

    #[test]
    fn no_list_draws_nothing() {
        let mut jobs = Jobs::default();
        let mut h = host();
        jobs.add(Work::new(5, 30, 0, WorkKind::Test(0)), 1 << 10, None);
        assert!(!jobs.assign_emergency(0, &mut h));
        assert_eq!(h.rng.rand(), Random::new(7).rand());
    }
}
