//! The job manager (`Game`). See docs/JOBS.md.
//!
//! Every `Work` gets a sequential id (from 1) and a priority (sequential unless the owner keeps
//! one). The manager indexes works by id, by priority, by category (`+ (c − 1)·0x10`, categories
//! 1..14 from the registration mask's bits) and by profession (the work's slot). All of them are
//! trees keyed by id, so iteration is in id order.
//!
//! The AI's work (9) and emergency (2) activities ask the manager for a job:
//!
//! - [`Jobs::assign_work`]: the citizen's own profession's works, then [`Jobs::assign_emergency`],
//!   then the laborers' works;
//! - [`Jobs::assign_emergency`]: category lists chosen from low stock (`Rand(n)` on the game
//!   stream), each tried by distance;
//! - `try_list` sorts candidate works by priority or path rank, ties by id, and `offer` offers them
//!   in order.
//!
//! A work's virtual slots are supplied by a [`WorkHost`]: try to assign (0 no, 1 not now, 2 taken),
//! a second try, a task completed, a task failed, cancel every worker, refresh.
//!
//! Original: `FUN_1400fe650`, `FUN_1400fcd00`, `FUN_1400fca20`, `FUN_1400fce80`, `FUN_1400fd4d0`,
//! `FUN_1400fd160`, `FUN_1400fd030`.
mod offers;
mod queue;

// Game +0x118; +0x00; +0x04; +0x30; +0x10; +0x120; +0x40 + (c − 1)·0x10; +0x50; +0x20; +0x28;
// +0x38; +0x40; +0x48

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
    // vtable 0x140167840
    /// `GatherRawMaterials`.
    ///
    /// Original: `FUN_1400f1be0`.
    Gather(crate::work::construction::Gather),
    // vtable 0x1401674c0
    /// `BuildStructure`.
    ///
    /// Original: `FUN_1400eae90`.
    Build(crate::work::construction::BuildWork),
    // vtable 0x140167540
    /// `ClearArea`.
    ///
    /// Original: `FUN_1400ebe30`.
    Clear(crate::work::construction::ClearWork),
    // vtable 0x140167c30
    /// `Pickup`: carry a pile to storage. A pile makes it when it is activated.
    ///
    /// Original: made by `FUN_14012e6a0`.
    Pickup(crate::work::construction::PickupWork),
    // vtable 0x140167640; +0x90
    /// `ConsumeProduce`: a workplace's production.
    ///
    /// Original: `FUN_1400ee6d0`; made by slot `FUN_14011e1d0`.
    Produce(crate::work::construction::ProduceWork),
    // vtable 0x140167ab0
    /// `LocationWork`: staff stand at a school, chapel or doctor.
    Location(crate::people::services::LocationWork),
    // vtable 0x1401679c0
    /// The work of gatherers and herbalists.
    ///
    /// Original: made by `FUN_14004bac0`.
    Herbalist(crate::production::herbalist::HerbalistWork),
    // vtable 0x1401677c0
    /// A forester's work, with the same fields.
    ///
    /// Original: made by `FUN_140124c50`.
    Forester(crate::production::herbalist::HerbalistWork),
    // vtable 0x140167a40
    /// A hunter's work, with the gatherer's fields.
    ///
    /// Original: made by `FUN_140127b70`.
    Hunt(crate::production::herbalist::HerbalistWork),
    // vtable 0x140167cb0
    /// `PlantArea`.
    ///
    /// Original: `FUN_1400f9b70`.
    PlantArea(crate::production::farm::AreaWork),
    // vtable 0x140167db0
    /// `TendArea`: the workers, as the gatherer's.
    TendArea(crate::production::herbalist::HerbalistWork),
    // vtable 0x1401678c0
    /// The harvest work.
    ///
    /// Original: `FUN_1400f2990`.
    Harvest(crate::production::farm::AreaWork),
    // vtable 0x140167d30
    /// `PlantOrchard`: `need` marks the slots being planted.
    ///
    /// Original: `FUN_1400fa610`.
    PlantOrchard(crate::production::farm::AreaWork),
    // vtable 0x140167940
    /// Fruit picking.
    ///
    /// Original: `FUN_1400f3530`.
    PickFruit(crate::production::farm::AreaWork),
    // vtable 0x140167bc0; +0x48
    /// PastureComponent's work: the ranchers tending, as the gatherer's.
    ///
    /// Original: `FUN_1400f8250`.
    Pasture(crate::production::herbalist::HerbalistWork),
    // vtable 0x1401675c0; +0x58
    /// `ClearStorage`: an empty house's goods go to the town's storage; the haulers.
    ///
    /// Original: `FUN_1400ec920`.
    ClearStorage(crate::production::herbalist::HerbalistWork),
    // vtable 0x1401676c0; +0x50
    /// `FireWork`: firefighters carrying buckets to a burning building; the workers.
    ///
    /// Original: `FUN_1400ef0a0`.
    Fire(crate::production::herbalist::HerbalistWork),
    // vtable 0x140167e30
    /// `TradeWork`: haulers keeping the trading post's stock.
    Trade(crate::commerce::trade::TradeWork),
    // vtable 0x140167430
    /// `BuildRoad`: one cell of road.
    Road(crate::work::road::RoadWork),
    // vtable 0x140167b40; +0x90
    /// MarketWork (made by MarketComponent's slot).
    Market(crate::commerce::market::MarketWork),
    // vtable 0x140167740
    /// `FishingWork`.
    ///
    /// Original: `FUN_1400eff90`.
    Fishing(crate::production::fishing::FishingWork),
    /// A test stand-in.
    Test(i32),
}

// vtable 0x140167e30
/// `Work`.
///
/// Original: `FUN_1400fc7b0`.
#[derive(Debug, Clone)]
pub struct Work {
    // +0x10
    /// The owning entity.
    pub entity: usize,
    // +0x20; +0x24
    /// The id.
    pub id: i32,
    /// The priority.
    pub priority: i32,
    // +0x28
    /// The owner's map-rectangle centre cell.
    pub cell: i32,
    // +0x2c
    /// The manager pass in which it last answered "not now".
    pub failed_pass: i32,
    // +0x30, +0x38, +0x40
    /// The clock of the distance patience: how long a far work waits before a citizen takes it.
    ///
    /// Original: used by `FUN_1400fc3c0`.
    pub patience: i64,
    /// The clock when the patience was last brought up to date.
    pub patience_base: i64,
    /// How many times a citizen found the work too far.
    pub far_count: i32,
    // +0x50
    /// The profession whose citizens do this work.
    pub profession: usize,
    /// What kind of work it is.
    pub kind: WorkKind,
}

impl Work {
    /// `cell` is `(y + h/2)·n + x + w/2` of the owner's map rectangle.
    ///
    /// Original: `FUN_1400fc7b0`.
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
    /// The citizen's workplace.
    fn workplace(&self, citizen: usize) -> Option<usize>;
    /// The citizen's map cell.
    fn cell(&self, citizen: usize) -> i32;
    /// Whether there is an emergency near the citizen.
    fn has_emergency(&self, citizen: usize) -> bool;
    /// Profession `+0x4c`.
    fn works_anywhere(&self, profession: usize) -> bool;
    /// `FUN_1400a4ce0(map, a, b, 1)`, with the callers' fallback `n + (int)sqrtf(d²)`.
    fn rank(&mut self, a: i32, b: i32) -> i32;
    /// The stock manager's `(count, limit)` per slot (`Game +0xf0 → +0x170`).
    fn stock(&self, slot: usize) -> (i32, i32);
    /// The laborer and builder professions (`Game +0xb0 → +0x30/+0x38`).
    fn laborer(&self) -> usize;
    /// The builder profession.
    fn builder(&self) -> usize;
    /// The game's random numbers.
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
    /// The work.
    pub work: i32,
    /// The citizen.
    pub citizen: usize,
    /// The task's parameter.
    pub param: i32,
    /// Whether the task completed (else it was aborted).
    pub complete: bool,
}

/// The manager.
#[derive(Debug, Clone)]
pub struct Jobs {
    /// `+0x00`: last id (wraps from `0x7ffffffe` to 0); `+0x04`: next sequential priority.
    next_id: i32,
    next_priority: i32,
    // +0x08
    /// Pass counter (one per simulation iteration).
    ///
    /// Original: `FUN_14009b850`.
    pub pass: i32,
    /// `+0x10`: priority → id.
    priorities: BTreeMap<i32, i32>,
    // +0x30
    /// Id → work.
    pub works: BTreeMap<i32, Work>,
    /// `+0x40..`: category 1..14 → ids.
    categories: [BTreeSet<i32>; CATEGORIES],
    /// `+0x120`: profession → ids.
    professions: BTreeMap<usize, BTreeSet<i32>>,
    // +0x130
    /// Manager clock (ticks), advanced with the pass.
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
    /// One simulation iteration of the job manager.
    ///
    /// Original: `FUN_1400fe650`, and the step in `FUN_14009b850`.
    pub fn step(&mut self, dt: i64) {
        self.pass = self.pass.wrapping_add(1);
        self.time = self.time.wrapping_add(dt);
    }

    /// Whether there is no work.
    pub fn is_empty(&self) -> bool {
        self.works.is_empty()
    }

    /// Register a work. With `keep`, the work gets that priority (moved down past any value already
    /// taken), else the next one in sequence. Returns the work's id.
    ///
    /// Original: `FUN_1400fdd70(work, mask, keep, priority)`.
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

    // +0x130
    /// A saved manager: its counters, then each work with the categories it was filed under
    /// (1..14), and the professions its profession map holds. The priority index is rebuilt from
    /// the works' own priorities, as the load pass does; the clock is not saved and restarts at 0.
    ///
    /// Original: `FUN_1400fe710`.
    pub fn restore(
        &mut self,
        next_id: i32,
        next_priority: i32,
        pass: i32,
        works: impl IntoIterator<Item = (Work, Vec<u32>)>,
        professions: &[usize],
    ) {
        *self = Jobs {
            next_id,
            next_priority,
            pass,
            ..Default::default()
        };
        // The profession map keeps a profession whose works are all gone.
        for &p in professions {
            self.professions.entry(p).or_default();
        }
        for (work, categories) in works {
            let id = work.id;
            self.priorities.entry(work.priority).or_insert(id);
            for c in categories {
                if let Some(set) = (c as usize)
                    .checked_sub(1)
                    .and_then(|c| self.categories.get_mut(c))
                {
                    set.insert(id);
                }
            }
            self.professions
                .entry(work.profession)
                .or_default()
                .insert(id);
            self.works.insert(id, work);
        }
    }

    // +0x00; +0x04
    /// The counters a save holds.
    pub fn counters(&self) -> (i32, i32) {
        (self.next_id, self.next_priority)
    }

    /// The works filed under category `c` (1..14), in id order.
    pub fn category(&self, c: u32) -> Vec<i32> {
        (c as usize)
            .checked_sub(1)
            .and_then(|c| self.categories.get(c))
            .map(|set| set.iter().copied().collect())
            .unwrap_or_default()
    }

    // +0x120
    /// The works by profession, in profession then id order.
    pub fn by_profession(&self) -> Vec<(usize, Vec<i32>)> {
        self.professions
            .iter()
            .map(|(&p, ids)| (p, ids.iter().copied().collect()))
            .collect()
    }

    /// A task finished (the start of `CompleteTask`): remove the work when its slot says it is
    /// done.
    ///
    /// Original: `FUN_1400fdc30`.
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

    /// A task failed (`CompleteTask` cancelled).
    ///
    /// Original: `FUN_1400fdcd0`.
    pub fn fail(&mut self, id: i32, citizen: usize, param: i32, host: &mut impl WorkHost) {
        if let Some(mut work) = self.works.remove(&id) {
            host.fail(&mut work, citizen, param);
            self.works.insert(id, work);
            self.drain(host);
        }
    }
}

#[cfg(test)]
mod tests;
