//! Professions (`Game`), workplaces (WorkPlaceComponent, the town manager's workplace registry
//! `Game →`) and who takes a job when the player changes a profession's count. See docs/JOBS.md.
//!
//! The ProfessionUI spinners and the WorkPlaceUI buttons call the town manager directly in the
//! original, between simulation iterations; the port queues them as [`Command`]s that the
//! simulation applies before its next iteration, which is the same point in the game's Random
//! stream.
//!
//! Original: `FUN_14007b890`, `FUN_14008ccb0`.
mod commands;
mod hiring;
mod settings;
pub use commands::Command;

// Game +0xb0; Game +0xf0 → +0x100

use std::collections::BTreeMap;

use crate::people::life::Home;
use crate::random::Random;
use crate::work::stock::StorageSearchMap;

/// What the workplace searches read.
///
/// Original: `FUN_140115490`, `FUN_140115940`, `FUN_140115eb0`.
pub trait Search {
    /// Citizen `c`'s map cell.
    fn cell(&self, c: usize) -> i32;
    /// Profession `+0x4c` ([`Professions::works_anywhere`]).
    fn anywhere(&self, profession: usize) -> bool;
    /// The workplace's work priority (`Jobs` entry `+0x24`, lower first).
    fn priority(&self, workplace: usize) -> i32;
    /// `FUN_14011ada0`: the workplace's entrance can be reached from `cell`.
    fn reach(&mut self, cell: i32, workplace: usize) -> bool;
    /// The game's random generator (the reachability test and the hiring draw from it).
    fn rng(&mut self) -> &mut Random;
}

/// `Profession` fields the simulation reads.
///
/// Original: `FUN_1400ae3e0`.
#[derive(Debug, Clone, Default)]
pub struct Profession {
    /// `Game\Profession\Profession.rsc:<key>`.
    pub reference: String,
    // +0x30; +0x20
    /// Name key (`ProfessionBuilder`), in the string table.
    pub name: String,
    /// The string table of the name, if not the default one.
    pub string_table: Option<String>,
    // +0x28; +0x30
    /// The icon's sprite sheet (sprite).
    pub sprite_sheet: Option<String>,
    // +0x4c
    /// Works anywhere (laborers and builders): `Jobs::TryList` offers it work outside its
    /// workplace, and the job status icon stays off without one.
    ///
    /// Original: `FUN_1400fce80`.
    pub works_anywhere: bool,
    // +0x4d
    /// With a workplace, the citizen does not catch diseases (the doctor).
    ///
    /// Original: `FUN_14011c060`.
    pub immune_at_work: bool,
    // +0x40
    /// The death-message key of an accident at its work.
    pub death: String,
    // +0x54, +0x58; +0x54; +0x58
    /// The accident odds `(years, factor)`: on average one accident per `years × factor` years
    /// at work; none when the factor is not positive.
    ///
    /// Original: `FUN_1400ae760`.
    pub accident: (f32, f32),
    // +0x50
    /// How much a citizen of this profession carries (100; 500 for traders and vendors).
    ///
    /// Original: `FUN_140114c70`.
    pub carry: i32,
}

// +0x20/+0x28; +0x30; +0x38; Game +0xb0; +0x40; +0x48
/// The profession manager (`Profession.rsc:resource`): the list of professions
/// (`Profession.rsc:professionList`), the laborer and builder entries, and the child and student
/// entries.
///
/// Original: `FUN_1400ae300`.
#[derive(Debug, Clone, Default)]
pub struct Professions {
    /// Every profession.
    pub list: Vec<Profession>,
    /// The laborer's index in `list`.
    pub laborer: usize,
    /// The builder's index in `list`.
    pub builder: usize,
}

impl Professions {
    /// Whether a profession works anywhere (its description says so), not at one workplace.
    pub fn works_anywhere(&self, p: usize) -> bool {
        self.list.get(p).is_some_and(|p| p.works_anywhere)
    }
}

// vtable 0x14016b160; +0x30
/// WorkPlaceComponent plus the entity's WorkComponent most-workers value.
///
/// Original: `FUN_1401347b0`.
#[derive(Debug, Clone)]
pub struct WorkPlace {
    // +0x48
    /// The profession its workers take (builder while under construction).
    pub profession: Option<usize>,
    // +0x20
    /// WorkPlaceDescription: the profession once built (none for houses and barns).
    pub description_profession: Option<usize>,
    // +0x50
    /// Accepting workers (the constructor sets it).
    pub enabled: bool,
    // +0x58/+0x60
    /// Citizen indices, in assignment order.
    pub workers: Vec<usize>,
    // +0x30
    /// WorkComponent: most workers; 0 until first read, and every reader recomputes it from the
    /// description while it is 0 (see [`WorkPlace::max`]).
    pub max_workers: i32,
    // WorkDescription +0x24, scaled when +0x34 is set.
    /// The default number of workers, from the work description; scaled by the footprint's area
    /// when the description says so.
    ///
    /// Original: `WorkComponent::MaxWorkers`, `FUN_140133a90`.
    pub default_max: i32,
    // +0xf0
    /// The building's StatusIconComponent bits ([`icon`]).
    pub status: u32,
}

/// The building status icons (`StatusIcons\BuildingIconSpriteSheet`): the bit of each.
pub mod icon {
    /// `BuildPaused`: switched off while being built.
    pub const BUILD_PAUSED: u8 = 0;
    /// `WorkDisabled`: switched off.
    pub const WORK_DISABLED: u8 = 1;
    /// `LimitProduction`: the stock is at its limit (or the building is full).
    pub const LIMIT: u8 = 2;
    /// `WorkUnavailable`: inputs are missing.
    pub const NO_WORK: u8 = 3;
    /// `WorkersUnavailable`: nobody works there.
    pub const NO_WORKERS: u8 = 4;
    /// `BuildDestroy`: marked to be torn down.
    pub const DESTROY: u8 = 5;
    /// A house without food, without firewood.
    pub const NO_FOOD: u8 = 6;
    /// The icon for no firewood.
    pub const NO_FIREWOOD: u8 = 7;
}

impl WorkPlace {
    /// A workplace with its profession and its default number of workers.
    pub fn new(description_profession: Option<usize>, default_max: i32) -> WorkPlace {
        WorkPlace {
            profession: None,
            description_profession,
            enabled: true,
            workers: Vec::new(),
            max_workers: 0,
            default_max,
            status: 0,
        }
    }

    /// Show or hide a status icon of the building.
    ///
    /// Original: `FUN_1401318c0` on the building's status icon component.
    pub fn set_icon(&mut self, bit: u8, on: bool) {
        crate::people::status::set_icon(&mut self.status, bit, on, false);
    }

    // +0x30; if +0x30 == 0 { +0x30 = MaxWorkers() }
    /// WorkComponent as every reader sees it: `if == 0 { = MaxWorkers }`.
    pub fn max(&self) -> i32 {
        if self.max_workers == 0 {
            self.default_max
        } else {
            self.max_workers
        }
    }
}

// +0x248; +0x238; +0x1f0
/// The citizen fields assignment reads and writes (Citizen).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Worker {
    // +0x208; desc+0x44 <= age
    /// `desc <= age` and no school: the citizens every search counts.
    pub adult: bool,
    /// The profession.
    pub profession: usize,
    /// The workplace, if any.
    pub workplace: Option<usize>,
    /// The home, if any.
    pub home: Option<usize>,
}

/// The town manager's workplace and residence registries and every WorkPlaceComponent.
#[derive(Debug, Clone, Default)]
pub struct Workforce {
    /// Entity → WorkPlaceComponent.
    pub workplaces: BTreeMap<usize, WorkPlace>,
    // +0x100
    /// Profession → workplace entities, in registration order.
    ///
    /// Original: `FUN_14008ea70`.
    pub registry: BTreeMap<usize, Vec<usize>>,
    // +0x20/+0x28; FUN_14012f2b0 appends a residence when it is built.
    /// The residences, in the order they were built.
    pub residences: Vec<usize>,
    /// Workplaces being torn down (their state machine in 3 to 5), kept by the sites' update: the
    /// last worker leaving one shows no `WorkersUnavailable`.
    ///
    /// Original: `FUN_140123be0`.
    pub demolishing: std::collections::BTreeSet<usize>,
}

impl Workforce {
    /// Register `entity` under `profession` once.
    ///
    /// Original: `FUN_14008ea70`.
    pub fn register(&mut self, entity: usize, profession: usize) {
        let list = self.registry.entry(profession).or_default();
        if !list.contains(&entity) {
            list.push(entity);
        }
    }

    /// Remove a workplace from a profession's list (every occurrence), keeping the order.
    ///
    /// Original: `FUN_140002cc0`.
    pub fn unregister(&mut self, entity: usize, profession: usize) {
        if let Some(list) = self.registry.get_mut(&profession) {
            list.retain(|&e| e != entity);
        }
    }

    /// Move citizen `c` to workplace `wp` (removing it from its old one, then appending to the new
    /// one).
    ///
    /// Original: `FUN_140134cb0`, `FUN_140134ef0`, `FUN_1401173b0`.
    pub fn set_workplace(&mut self, workers: &mut [Worker], c: usize, wp: Option<usize>) {
        self.move_worker(c, workers[c].workplace, wp);
        workers[c].workplace = wp;
    }

    /// The workplaces' side of [`Workforce::set_workplace`]: `c` leaves `old` and joins `new`
    /// (not while it is being torn down). When the last worker leaves, the limit and work icons go,
    /// and an enabled workplace of its own profession shows `WorkersUnavailable`; joining removes
    /// that icon. The entity events 2 and 3 (work sounds and smoke) are only for presentation; the
    /// app drives them from the workers.
    ///
    /// Original: `FUN_140123be0`; leaving `FUN_140134cb0`, joining `FUN_140134ef0`.
    pub fn move_worker(&mut self, c: usize, old: Option<usize>, new: Option<usize>) {
        if let Some(old) = old {
            let demolishing = self.demolishing.contains(&old);
            if let Some(w) = self.workplaces.get_mut(&old) {
                w.workers.retain(|&x| x != c);
                if w.workers.is_empty() {
                    w.set_icon(icon::LIMIT, false);
                    w.set_icon(icon::NO_WORK, false);
                    if w.enabled
                        && !demolishing
                        && w.profession.is_some()
                        && w.profession == w.description_profession
                    {
                        w.set_icon(icon::NO_WORKERS, true);
                    }
                }
            }
        }
        if let Some(new) = new {
            if let Some(w) = self.workplaces.get_mut(&new) {
                w.set_icon(icon::NO_WORKERS, false);
                w.workers.push(c);
            }
        }
    }

    /// The assignment tail shared by every search: change the workplace and profession only
    /// when either differs.
    fn assign(&mut self, workers: &mut [Worker], c: usize, profession: usize, wp: Option<usize>) {
        if workers[c].profession != profession || workers[c].workplace != wp {
            self.set_workplace(workers, c, wp);
            workers[c].profession = profession;
        }
    }

    /// `FUN_14008e940`: shuffle a profession's workplace list (`Rand(n)` per position, when it
    /// has at least two).
    fn shuffle_workplaces(&mut self, profession: usize, rng: &mut Random) {
        if let Some(list) = self.registry.get_mut(&profession) {
            shuffle(list, rng);
        }
    }
}

/// For each position `i`, swap with `Rand(n)` (no draws below two entries).
///
/// Original: `FUN_14008e940`, `FUN_14008eb70`.
pub fn shuffle<T>(list: &mut [T], rng: &mut Random) {
    let n = list.len();
    if n < 2 {
        return;
    }
    for i in 0..n {
        let j = rng.rand_below(n as u32) as usize;
        if i != j {
            list.swap(i, j);
        }
    }
}

#[cfg(test)]
mod tests;
