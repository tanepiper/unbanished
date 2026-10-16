//! Professions (`Game +0xb0`), workplaces (WorkPlaceComponent, the town manager's workplace
//! registry `Game +0xf0 → +0x100`) and who takes a job when the player changes a profession's
//! count. See docs/JOBS.md.
//!
//! The ProfessionUI spinners (`FUN_14007b890`) and the WorkPlaceUI buttons (`FUN_14008ccb0`)
//! call the town manager directly in the original, between simulation iterations; the port
//! queues them as [`Command`]s that the simulation applies before its next iteration, which is
//! the same point in the game's Random stream.

use std::collections::BTreeMap;

use crate::life::Home;
use crate::random::Random;
use crate::stock::StorageSearchMap;

/// What the workplace searches read (`FUN_140115490`, `FUN_140115940`, `FUN_140115eb0`).
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

/// `Profession` (serializer `FUN_1400ae3e0`) fields the simulation reads.
#[derive(Debug, Clone, Default)]
pub struct Profession {
    /// `Game\Profession\Profession.rsc:<key>`.
    pub reference: String,
    /// `+0x30`: name key (`ProfessionBuilder`), in the `+0x20` string table.
    pub name: String,
    pub string_table: Option<String>,
    /// `+0x28`: the icon's sprite sheet (sprite `+0x30`).
    pub sprite_sheet: Option<String>,
    /// `+0x4c`: works anywhere (laborers and builders): `Jobs::TryList` (`FUN_1400fce80`) offers
    /// it work outside its workplace, and the job status icon stays off without one.
    pub works_anywhere: bool,
    /// `+0x40`: the death-message key of an accident at its work.
    pub death: String,
    /// `+0x54`, `+0x58`: accident odds (`FUN_1400ae760`): on average one per `+0x54` years ×
    /// `+0x58` of time between checks; none when `+0x58` is not positive.
    pub accident: (f32, f32),
}

/// The profession manager (`Game +0xb0`, `Profession.rsc:resource`, serializer
/// `FUN_1400ae300`): the list (`+0x20/+0x28`, `Profession.rsc:professionList`) and the
/// laborer (`+0x30`) and builder (`+0x38`) entries; `+0x40`/`+0x48` are child and student.
#[derive(Debug, Clone, Default)]
pub struct Professions {
    pub list: Vec<Profession>,
    pub laborer: usize,
    pub builder: usize,
}

impl Professions {
    pub fn works_anywhere(&self, p: usize) -> bool {
        self.list.get(p).is_some_and(|p| p.works_anywhere)
    }
}

/// WorkPlaceComponent (vtable `0x14016b160`, constructor `FUN_1401347b0`) plus the entity's
/// WorkComponent most-workers value (`+0x30`).
#[derive(Debug, Clone)]
pub struct WorkPlace {
    /// `+0x48`: the profession its workers take (builder while under construction).
    pub profession: Option<usize>,
    /// WorkPlaceDescription `+0x20`: the profession once built (none for houses and barns).
    pub description_profession: Option<usize>,
    /// `+0x50`: accepting workers (the constructor sets it).
    pub enabled: bool,
    /// `+0x58/+0x60`: citizen indices, in assignment order.
    pub workers: Vec<usize>,
    /// WorkComponent `+0x30`: most workers; 0 until first read, and every reader recomputes
    /// it from the description while it is 0 (see [`WorkPlace::max`]).
    pub max_workers: i32,
    /// `FUN_140133a90` (WorkComponent::MaxWorkers): WorkDescription `+0x24`, scaled by the
    /// footprint area when `+0x34` is set.
    pub default_max: i32,
    /// The building's StatusIconComponent bits (`+0xf0`, [`icon`]).
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
    pub const NO_FIREWOOD: u8 = 7;
}

impl WorkPlace {
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

    /// `FUN_1401318c0` on the building's StatusIconComponent.
    pub fn set_icon(&mut self, bit: u8, on: bool) {
        crate::status::set_icon(&mut self.status, bit, on, false);
    }

    /// WorkComponent `+0x30` as every reader sees it: `if +0x30 == 0 { +0x30 = MaxWorkers() }`.
    pub fn max(&self) -> i32 {
        if self.max_workers == 0 {
            self.default_max
        } else {
            self.max_workers
        }
    }
}

/// The citizen fields assignment reads and writes (Citizen `+0x1f0`, `+0x248`, `+0x238`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Worker {
    /// `desc+0x44 <= age` and no school (`+0x208`): the citizens every search counts.
    pub adult: bool,
    pub profession: usize,
    pub workplace: Option<usize>,
    pub home: Option<usize>,
}

/// A player action on professions, applied by the simulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// ProfessionUI spinner `300 + profession` set to `count` (`FUN_14007b890`).
    SetCount { profession: usize, count: i32 },
    /// WorkPlaceUI `0x1450`: one more worker of the workplace's profession (`FUN_1400918a0`).
    AddWorker { entity: usize },
    /// WorkPlaceUI `0x1451`: one fewer (`FUN_140096650`).
    RemoveWorker { entity: usize },
    /// WorkPlaceUI `0x1452/0x1453`: most workers ± 1 (`FUN_14008cae0` → `FUN_140134fd0`).
    ChangeMaxWorkers { entity: usize, delta: i32 },
    /// BuildUI's pause button (element 6000, `FUN_1400b7370`): `FUN_1401350d0`.
    SetEnabled { entity: usize, enabled: bool },
    /// CropUI's crop list (slot `+0x58`, `FUN_1400b7df0`): the field's crop; choosing the
    /// first one switches farming on.
    SetCrop { entity: usize, crop: Option<usize> },
    /// The field's buttons (`FUN_140120190`): 0 plant now, 1 harvest now, 2 farming on/off.
    FieldAction { entity: usize, action: i32 },
    /// PastureUI's slider (slot `+0x48`, `FUN_1400ba2f0`): the most animals the pasture keeps.
    SetLimit { entity: usize, limit: i32 },
    /// Remove Buildings (`FUN_140109b40`) and DestroyUI: `FUN_140123470`.
    Demolish { entity: usize },
    /// DestroyUI `0x1771` (`FUN_1401238d0`): the demolition is called off.
    CancelDemolition { entity: usize },
    /// RoadTool's apply (`FUN_14010a200` → `FUN_1400a1e30`): plan a road on a cell.
    PlanRoad {
        cell: i32,
        kind: u8,
        ty: u8,
        material: Option<usize>,
    },
    /// Remove Roads (`FUN_140108ab0` → `FUN_1400a1bd0`): take a planned road back; a built one
    /// gets a removal marker (`marker`, the tool's template).
    ClearRoad { cell: i32, marker: usize },
    /// A StateMachine message (slot `+0x10`): DestroyComponent's Remove Cancel (0 while torn
    /// down), DestroyUI `0x1772` (`FUN_140123940`, 0: rebuild the rubble) and `0x1773`
    /// (`FUN_1401239a0`, 1: clear it); ImmigrationUI `0x9858` (7: take the nomads in) and
    /// `0x9859` (8: turn them away).
    StateMessage { entity: usize, message: i32 },
    /// ControlUI (`FUN_140060150`): the `kind` component's control `index` (slot `+0x70`).
    Control {
        entity: usize,
        kind: u32,
        index: i32,
    },
    /// ConsumeProduceUI's product (`FUN_14005fc90`): ConsumeProduceComponent `+0x50`.
    SetRecipe { entity: usize, recipe: i32 },
}

/// The town manager's workplace and residence registries and every WorkPlaceComponent.
#[derive(Debug, Clone, Default)]
pub struct Workforce {
    /// Entity → WorkPlaceComponent.
    pub workplaces: BTreeMap<usize, WorkPlace>,
    /// `+0x100`: profession → workplace entities, in registration order (`FUN_14008ea70`).
    pub registry: BTreeMap<usize, Vec<usize>>,
    /// `+0x20/+0x28`: residence entities (`FUN_14012f2b0` appends on completion).
    pub residences: Vec<usize>,
}

impl Workforce {
    /// `FUN_14008ea70`: register `entity` under `profession` once.
    pub fn register(&mut self, entity: usize, profession: usize) {
        let list = self.registry.entry(profession).or_default();
        if !list.contains(&entity) {
            list.push(entity);
        }
    }

    /// `FUN_140002cc0` on the registry list: remove every occurrence, keeping order.
    pub fn unregister(&mut self, entity: usize, profession: usize) {
        if let Some(list) = self.registry.get_mut(&profession) {
            list.retain(|&e| e != entity);
        }
    }

    /// `FUN_1401173b0`: move citizen `c` to workplace `wp` (removing it from its old one,
    /// `FUN_140134cb0`, then appending to the new one, `FUN_140134ef0`).
    /// Not ported: the workplaces' StatusIconComponent bits and entity flags 2/3.
    pub fn set_workplace(&mut self, workers: &mut [Worker], c: usize, wp: Option<usize>) {
        self.move_worker(c, workers[c].workplace, wp);
        workers[c].workplace = wp;
    }

    /// The workplaces' side of [`Workforce::set_workplace`]: `c` leaves `old`
    /// (`FUN_140134cb0`: the last worker gone, the limit and work icons go, and an enabled
    /// workplace of its own profession shows `WorkersUnavailable`) and joins `new`
    /// (`FUN_140134ef0`: that icon goes).
    /// Not ported: the entity events 2 and 3 (work sounds, smoke) and the destroy check.
    pub fn move_worker(&mut self, c: usize, old: Option<usize>, new: Option<usize>) {
        if let Some(old) = old {
            if let Some(w) = self.workplaces.get_mut(&old) {
                w.workers.retain(|&x| x != c);
                if w.workers.is_empty() {
                    w.set_icon(icon::LIMIT, false);
                    w.set_icon(icon::NO_WORK, false);
                    if w.enabled
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

    /// `FUN_1400925d0`: shuffle, then the first enabled workplace with a free place.
    pub fn free_workplace(&mut self, profession: usize, rng: &mut Random) -> Option<usize> {
        self.shuffle_workplaces(profession, rng);
        let list = self.registry.get(&profession)?;
        list.iter().copied().find(|e| {
            self.workplaces
                .get(e)
                .is_some_and(|w| (w.workers.len() as i32) < w.max() && w.enabled)
        })
    }

    /// `FUN_1400918a0` (and the loop body of `FUN_14007b890`): make one laborer take
    /// `profession` at `wp`. Searches, in order: a laborer sharing a home with someone of that
    /// profession (`FUN_1400920c0`), the homed laborer nearest the workplace (`FUN_140091ef0`),
    /// the household with the most laborers (`FUN_140091ac0`), then the first laborer
    /// (`FUN_140091980`).
    #[allow(clippy::too_many_arguments)]
    pub fn hire(
        &mut self,
        workers: &mut [Worker],
        laborer: usize,
        profession: usize,
        wp: Option<usize>,
        homes: &BTreeMap<usize, Home>,
        map: &mut dyn StorageSearchMap,
        rng: &mut Random,
    ) -> Option<usize> {
        if let Some(c) = self.family_member(workers, laborer, profession, wp, homes, map, rng) {
            return Some(c);
        }
        if let Some(w) = wp {
            if let Some(c) = self.nearest_laborer(workers, laborer, profession, w, map, rng) {
                return Some(c);
            }
        }
        if let Some(c) = self.largest_family(workers, laborer, profession, wp, homes, rng) {
            return Some(c);
        }
        let c = workers
            .iter()
            .position(|w| w.adult && w.profession == laborer)?;
        self.assign(workers, c, profession, wp);
        Some(c)
    }

    /// `FUN_1400920c0`: a laborer in the household of a citizen who already has `profession`:
    /// with a workplace, the household whose home entrance is nearest it (ties to the later
    /// one); without, the first such laborer at once.
    #[allow(clippy::too_many_arguments)]
    fn family_member(
        &mut self,
        workers: &mut [Worker],
        laborer: usize,
        profession: usize,
        wp: Option<usize>,
        homes: &BTreeMap<usize, Home>,
        map: &mut dyn StorageSearchMap,
        rng: &mut Random,
    ) -> Option<usize> {
        let target = wp.map_or(-1, |w| map.entrance(w, rng));
        let mut best = 0x7ffffff;
        let mut found: Option<(usize, usize)> = None;
        let free =
            |workers: &[Worker], m: usize| workers[m].adult && workers[m].profession == laborer;
        for c in 0..workers.len() {
            if !(workers[c].adult && workers[c].profession == profession) {
                continue;
            }
            let Some(home) = workers[c].home else {
                continue;
            };
            let e = map.entrance(home, rng);
            let d = if target < 0 {
                best
            } else {
                let d = map.rank(e, target);
                if d > best {
                    continue;
                }
                d
            };
            let Some(h) = homes.get(&home) else { continue };
            for (slot, family) in h.families.iter().enumerate() {
                if let Some(&m) = family.iter().find(|&&m| free(workers, m)) {
                    if wp.is_none() {
                        self.assign(workers, m, profession, None);
                        return Some(m);
                    }
                    found = Some((home, slot));
                    best = d;
                }
            }
        }
        let (home, slot) = found?;
        let m = *homes[&home].families[slot]
            .iter()
            .find(|&&m| free(workers, m))?;
        self.assign(workers, m, profession, wp);
        Some(m)
    }

    /// `FUN_140091ef0`: the homed laborer whose home entrance is nearest the workplace's
    /// (path rank, ties to the later citizen).
    fn nearest_laborer(
        &mut self,
        workers: &mut [Worker],
        laborer: usize,
        profession: usize,
        wp: usize,
        map: &mut dyn StorageSearchMap,
        rng: &mut Random,
    ) -> Option<usize> {
        let target = map.entrance(wp, rng);
        let mut best = 0x7ffffff;
        let mut found = None;
        for (i, w) in workers.iter().enumerate() {
            if !(w.adult && w.profession == laborer) {
                continue;
            }
            let Some(home) = w.home else { continue };
            let e = map.entrance(home, rng);
            let d = map.rank(e, target);
            if d <= best {
                best = d;
                found = Some(i);
            }
        }
        let c = found?;
        self.assign(workers, c, profession, Some(wp));
        Some(c)
    }

    /// `FUN_140091ac0`: shuffle the residences (`FUN_14008eb70`), then the first laborer of
    /// the household with the most laborers (the first such household on ties).
    fn largest_family(
        &mut self,
        workers: &mut [Worker],
        laborer: usize,
        profession: usize,
        wp: Option<usize>,
        homes: &BTreeMap<usize, Home>,
        rng: &mut Random,
    ) -> Option<usize> {
        shuffle(&mut self.residences, rng);
        let free =
            |workers: &[Worker], m: usize| workers[m].adult && workers[m].profession == laborer;
        let mut most = 0;
        let mut found = None;
        for &e in &self.residences {
            let Some(h) = homes.get(&e) else { continue };
            for (slot, family) in h.families.iter().enumerate() {
                let n = family.iter().filter(|&&m| free(workers, m)).count();
                if most < n {
                    most = n;
                    found = Some((e, slot));
                }
            }
        }
        let (e, slot) = found?;
        let m = *homes[&e].families[slot]
            .iter()
            .find(|&&m| free(workers, m))?;
        self.assign(workers, m, profession, wp);
        Some(m)
    }

    /// `FUN_140096650`: one citizen of `profession` becomes a laborer: the first without a
    /// workplace, else the first worker of the workplace with the most workers, else the
    /// first of that profession.
    pub fn fire(
        &mut self,
        workers: &mut [Worker],
        laborer: usize,
        profession: usize,
    ) -> Option<usize> {
        if let Some(c) = workers
            .iter()
            .position(|w| w.adult && w.profession == profession && w.workplace.is_none())
        {
            self.assign(workers, c, laborer, None);
            return Some(c);
        }
        let mut most = 0;
        let mut chosen = None;
        for e in self.registry.get(&profession).into_iter().flatten() {
            let n = self.workplaces.get(e).map_or(0, |w| w.workers.len());
            if most < n {
                most = n;
                chosen = Some(*e);
            }
        }
        if let Some(e) = chosen {
            let c = self.workplaces[&e].workers[0];
            self.assign(workers, c, laborer, None);
            return Some(c);
        }
        let c = workers
            .iter()
            .position(|w| w.adult && w.profession == profession)?;
        self.assign(workers, c, laborer, None);
        Some(c)
    }

    /// `FUN_14007ba70`'s counts: adults per profession.
    pub fn counts(&self, workers: &[Worker], professions: usize) -> Vec<i32> {
        let mut out = vec![0; professions];
        for w in workers.iter().filter(|w| w.adult) {
            if let Some(n) = out.get_mut(w.profession) {
                *n += 1;
            }
        }
        out
    }

    /// `FUN_14007ba70`'s "of N": the most workers of every enabled workplace of a profession.
    pub fn capacity(&self, profession: usize) -> i32 {
        self.registry
            .get(&profession)
            .into_iter()
            .flatten()
            .filter_map(|e| self.workplaces.get(e))
            .filter(|w| w.enabled)
            .map(|w| w.max())
            .sum()
    }

    /// `FUN_14007b890`: move the profession's count toward `count`, one citizen at a time.
    pub fn set_count(
        &mut self,
        workers: &mut [Worker],
        professions: &Professions,
        profession: usize,
        count: i32,
        homes: &BTreeMap<usize, Home>,
        map: &mut dyn StorageSearchMap,
        rng: &mut Random,
    ) {
        let current = self.counts(workers, professions.list.len())[profession];
        for _ in current..count {
            let wp = self.free_workplace(profession, rng);
            self.hire(
                workers,
                professions.laborer,
                profession,
                wp,
                homes,
                map,
                rng,
            );
        }
        for _ in count..current {
            self.fire(workers, professions.laborer, profession);
        }
    }

    /// `FUN_140134fd0`: set the most workers, then the first workers move to another
    /// workplace ([`Workforce::find_workplace`], or none) until they fit.
    pub fn set_max_workers(
        &mut self,
        workers: &mut [Worker],
        entity: usize,
        max: i32,
        search: &mut dyn Search,
    ) {
        let Some(w) = self.workplaces.get_mut(&entity) else {
            return;
        };
        w.max_workers = max;
        while max < self.workplaces[&entity].workers.len() as i32 {
            let c = self.workplaces[&entity].workers[0];
            let wp = self.find_workplace(c, workers[c].profession, search);
            self.set_workplace(workers, c, wp);
        }
    }

    /// `FUN_1401350d0`: accept workers or not; a disabled workplace's workers move to another
    /// ([`Workforce::find_workplace`], or none). Switched off it shows `BuildPaused` while
    /// being `building` (states 0, 1, 8), else `WorkDisabled`, and neither on; switched off it
    /// drops the limit, work and workers icons, on without workers of its own profession it
    /// shows `WorkersUnavailable`.
    pub fn set_enabled(
        &mut self,
        workers: &mut [Worker],
        entity: usize,
        enabled: bool,
        building: bool,
        search: &mut dyn Search,
    ) {
        let Some(w) = self.workplaces.get_mut(&entity) else {
            return;
        };
        w.enabled = enabled;
        if !enabled {
            let bit = if building {
                icon::BUILD_PAUSED
            } else {
                icon::WORK_DISABLED
            };
            w.set_icon(bit, true);
            for bit in [icon::LIMIT, icon::NO_WORK, icon::NO_WORKERS] {
                w.set_icon(bit, false);
            }
        } else {
            w.set_icon(icon::WORK_DISABLED, false);
            w.set_icon(icon::BUILD_PAUSED, false);
            if w.workers.is_empty()
                && w.profession.is_some()
                && w.profession == w.description_profession
            {
                w.set_icon(icon::NO_WORKERS, true);
            }
        }
        if !enabled {
            while let Some(&c) = self.workplaces.get(&entity).and_then(|w| w.workers.first()) {
                let wp = self.find_workplace(c, workers[c].profession, search);
                self.set_workplace(workers, c, wp);
            }
        }
    }

    /// `FUN_140115450`: a workplace of citizen `c`'s profession, enabled, with room and
    /// reachable: for a profession that works anywhere the one whose work comes first
    /// (`FUN_140115940`, the candidates sorted by work priority with `FUN_140115800`), else
    /// the one with the most room, the first of equals (`FUN_140115490`).
    pub fn find_workplace(&self, c: usize, p: usize, search: &mut dyn Search) -> Option<usize> {
        let list = self.registry.get(&p)?;
        let anywhere = search.anywhere(p);
        let cell = search.cell(c);
        let mut best = None;
        let mut most = 0;
        let mut first = i32::MAX;
        for &e in list {
            let Some(w) = self.workplaces.get(&e) else {
                continue;
            };
            let free = w.max() - w.workers.len() as i32;
            if free <= 0 || !w.enabled || !search.reach(cell, e) {
                continue;
            }
            if anywhere {
                let priority = search.priority(e);
                if best.is_none() || priority < first {
                    first = priority;
                    best = Some(e);
                }
            } else if most < free {
                most = free;
                best = Some(e);
            }
        }
        best
    }

    /// `FUN_140115eb0`: should citizen `c` look again? A profession that works anywhere when
    /// its workplace cannot be reached or another with room has work that comes first; the
    /// others when the reachable workplaces' room differs by more than one.
    pub fn should_move(
        &self,
        c: usize,
        p: usize,
        current: Option<usize>,
        search: &mut dyn Search,
    ) -> bool {
        let list = self.registry.get(&p).cloned().unwrap_or_default();
        let cell = search.cell(c);
        if !search.anywhere(p) {
            let (mut most, mut least) = (0, 0xffff);
            for e in list {
                let Some(w) = self.workplaces.get(&e) else {
                    continue;
                };
                if !w.enabled || !search.reach(cell, e) {
                    continue;
                }
                let free = w.max() - w.workers.len() as i32;
                if most < free {
                    most = free;
                }
                if free < least {
                    least = free;
                }
            }
            return 1 < most - least;
        }
        let Some(current) = current else {
            return false;
        };
        let priority = search.priority(current);
        if !search.reach(cell, current) {
            return true;
        }
        list.into_iter().any(|e| {
            e != current
                && self
                    .workplaces
                    .get(&e)
                    .is_some_and(|w| w.enabled && (w.workers.len() as i32) < w.max())
                && search.priority(e) < priority
        })
    }

    /// `FUN_140134330`: every worker leaves.
    pub fn clear_workers(&mut self, workers: &mut [Worker], entity: usize) {
        while let Some(&c) = self.workplaces.get(&entity).and_then(|w| w.workers.first()) {
            self.set_workplace(workers, c, None);
        }
    }

    /// Apply a UI command.
    pub fn apply(
        &mut self,
        command: Command,
        workers: &mut [Worker],
        professions: &Professions,
        homes: &BTreeMap<usize, Home>,
        map: &mut dyn StorageSearchMap,
        search: &mut dyn Search,
    ) {
        match command {
            Command::SetCount { profession, count } => self.set_count(
                workers,
                professions,
                profession,
                count,
                homes,
                map,
                search.rng(),
            ),
            Command::AddWorker { entity } => {
                let Some(p) = self.workplaces.get(&entity).and_then(|w| w.profession) else {
                    return;
                };
                let wp = self.free_workplace(p, search.rng());
                self.hire(
                    workers,
                    professions.laborer,
                    p,
                    wp,
                    homes,
                    map,
                    search.rng(),
                );
            }
            Command::RemoveWorker { entity } => {
                if let Some(p) = self.workplaces.get(&entity).and_then(|w| w.profession) {
                    self.fire(workers, professions.laborer, p);
                }
            }
            // The simulation answers this one with the building's state
            // ([`crate::town::Sim::enable_workplace`]).
            Command::SetEnabled { entity, enabled } => {
                self.set_enabled(workers, entity, enabled, false, search)
            }
            Command::ChangeMaxWorkers { entity, delta } => {
                if let Some(w) = self.workplaces.get(&entity) {
                    let max = w.max() + delta;
                    self.set_max_workers(workers, entity, max, search);
                }
            }
            // Handled by the crop field, orchard and pasture (`crate::farm`, `crate::pasture`).
            Command::SetCrop { .. }
            | Command::FieldAction { .. }
            | Command::SetLimit { .. }
            | Command::Demolish { .. }
            | Command::CancelDemolition { .. }
            | Command::StateMessage { .. }
            | Command::PlanRoad { .. }
            | Command::ClearRoad { .. }
            | Command::Control { .. }
            | Command::SetRecipe { .. } => {}
        }
    }
}

/// `FUN_14008e940` / `FUN_14008eb70`: for each position `i`, swap with `Rand(n)` (no draws
/// below two entries).
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
mod tests {
    use super::*;

    struct Flat;
    impl StorageSearchMap for Flat {
        fn entrance(&mut self, entity: usize, _rng: &mut Random) -> i32 {
            entity as i32
        }
        fn rank(&mut self, a: i32, b: i32) -> i32 {
            (a - b).abs()
        }
        fn market_position(&mut self, _: usize, _: i32, _: &mut Random) -> Option<(i32, [f32; 3])> {
            None
        }
        fn storage_position(&mut self, _: usize, _: i32) -> Option<(i32, [f32; 3])> {
            None
        }
    }

    fn professions() -> Professions {
        Professions {
            list: (0..3)
                .map(|i| Profession {
                    death: String::new(),
                    accident: (0.0, 0.0),
                    works_anywhere: i < 2,
                    ..Default::default()
                })
                .collect(),
            laborer: 0,
            builder: 1,
        }
    }

    #[test]
    fn spinner_promotes_the_first_laborers_and_demotes_unattached_first() {
        let p = professions();
        let mut workers = vec![
            Worker {
                adult: false,
                profession: 0,
                workplace: None,
                home: None,
            };
            1
        ];
        workers.extend((0..4).map(|_| Worker {
            adult: true,
            profession: 0,
            workplace: None,
            home: None,
        }));
        let mut force = Workforce::default();
        force.workplaces.insert(10, WorkPlace::new(None, 1));
        force.workplaces.get_mut(&10).unwrap().profession = Some(1);
        force.register(10, 1);
        let mut rng = Random::new(1);
        force.set_count(
            &mut workers,
            &p,
            1,
            2,
            &BTreeMap::new(),
            &mut Flat,
            &mut rng,
        );
        // The first builder fills the site's free place; the second has no workplace.
        assert_eq!(workers[1].profession, 1);
        assert_eq!(workers[1].workplace, Some(10));
        assert_eq!(workers[2].profession, 1);
        assert_eq!(workers[2].workplace, None);
        assert_eq!(force.counts(&workers, 3), vec![2, 2, 0]);
        assert_eq!(force.capacity(1), 1);
        // Demotion takes the builder without a workplace first.
        force.set_count(
            &mut workers,
            &p,
            1,
            1,
            &BTreeMap::new(),
            &mut Flat,
            &mut rng,
        );
        assert_eq!(workers[2].profession, 0);
        assert_eq!(workers[1].profession, 1);
        force.set_count(
            &mut workers,
            &p,
            1,
            0,
            &BTreeMap::new(),
            &mut Flat,
            &mut rng,
        );
        assert_eq!(workers[1].profession, 0);
        assert_eq!(workers[1].workplace, None);
        assert!(force.workplaces[&10].workers.is_empty());
    }

    /// Every workplace reachable except entity 99; priorities by entity.
    struct Open(Random);
    impl Search for Open {
        fn cell(&self, c: usize) -> i32 {
            c as i32
        }
        fn anywhere(&self, profession: usize) -> bool {
            profession == 1
        }
        fn priority(&self, workplace: usize) -> i32 {
            [0, 30, 20, 10][workplace % 4]
        }
        fn reach(&mut self, _cell: i32, workplace: usize) -> bool {
            workplace != 99
        }
        fn rng(&mut self) -> &mut Random {
            &mut self.0
        }
    }

    fn workforce(places: &[(usize, usize, i32, i32)]) -> Workforce {
        let mut w = Workforce::default();
        for &(e, p, max, workers) in places {
            let mut wp = WorkPlace::new(Some(p), max);
            wp.profession = Some(p);
            wp.workers = (100..100 + workers as usize).collect();
            w.workplaces.insert(e, wp);
            w.register(e, p);
        }
        w
    }

    #[test]
    fn workplace_searches() {
        let mut search = Open(Random::new(1));
        // Most room, the first of equals; full, disabled and unreachable ones are skipped.
        let mut w = workforce(&[(1, 2, 4, 3), (2, 2, 4, 1), (3, 2, 5, 2), (99, 2, 9, 0)]);
        assert_eq!(w.find_workplace(0, 2, &mut search), Some(2));
        w.workplaces.get_mut(&2).unwrap().enabled = false;
        assert_eq!(w.find_workplace(0, 2, &mut search), Some(3));
        // Works anywhere: the best priority with room.
        let w = workforce(&[(1, 1, 2, 0), (2, 1, 2, 0), (3, 1, 2, 2)]);
        assert_eq!(w.find_workplace(0, 1, &mut search), Some(2));
        // Balancing: move when the room differs by more than one.
        let w = workforce(&[(1, 2, 4, 4), (2, 2, 4, 3)]);
        assert!(!w.should_move(0, 2, Some(1), &mut search));
        let w = workforce(&[(1, 2, 4, 4), (2, 2, 4, 2)]);
        assert!(w.should_move(0, 2, Some(1), &mut search));
        // Works anywhere: move to work that comes first, or off an unreachable workplace.
        let w = workforce(&[(1, 1, 2, 1), (3, 1, 2, 0), (99, 1, 2, 1)]);
        assert!(w.should_move(0, 1, Some(1), &mut search));
        assert!(!w.should_move(0, 1, Some(3), &mut search));
        assert!(w.should_move(0, 1, Some(99), &mut search));
    }

    #[test]
    fn evicted_workers_move_on() {
        let mut search = Open(Random::new(1));
        let mut w = workforce(&[(1, 2, 4, 0), (2, 2, 4, 0)]);
        let mut workers: Vec<Worker> = (0..3)
            .map(|_| Worker {
                adult: true,
                profession: 2,
                workplace: None,
                home: None,
            })
            .collect();
        for c in 0..3 {
            w.set_workplace(&mut workers, c, Some(1));
        }
        w.set_max_workers(&mut workers, 1, 1, &mut search);
        assert_eq!(w.workplaces[&1].workers, vec![2]);
        assert_eq!(w.workplaces[&2].workers, vec![0, 1]);
        w.set_max_workers(&mut workers, 1, 2, &mut search);
        w.set_enabled(&mut workers, 2, false, false, &mut search);
        assert_eq!(w.workplaces[&1].workers, vec![2, 0]);
        assert!(w.workplaces[&2].workers.is_empty());
        assert_eq!(workers[1].workplace, None, "no room left: no workplace");
    }

    #[test]
    fn workplace_status_icons() {
        let mut search = Open(Random::new(1));
        let mut w = workforce(&[(1, 2, 2, 0)]);
        let mut workers: Vec<Worker> = (0..2)
            .map(|_| Worker {
                adult: true,
                profession: 2,
                workplace: None,
                home: None,
            })
            .collect();
        let bit = |w: &Workforce, b: u8| w.workplaces[&1].status >> b & 1 != 0;
        // Enabled without workers of its own profession: WorkersUnavailable.
        w.set_enabled(&mut workers, 1, true, false, &mut search);
        assert!(bit(&w, icon::NO_WORKERS));
        w.set_workplace(&mut workers, 0, Some(1));
        assert!(!bit(&w, icon::NO_WORKERS));
        w.workplaces
            .get_mut(&1)
            .unwrap()
            .set_icon(icon::LIMIT, true);
        // The last worker leaving drops the limit icon and shows the workers icon again.
        w.set_workplace(&mut workers, 0, None);
        assert!(!bit(&w, icon::LIMIT) && bit(&w, icon::NO_WORKERS));
        // Switched off: WorkDisabled (BuildPaused while being built), nothing else.
        w.set_enabled(&mut workers, 1, false, false, &mut search);
        assert_eq!(w.workplaces[&1].status, 1 << icon::WORK_DISABLED);
        w.set_enabled(&mut workers, 1, true, false, &mut search);
        w.set_enabled(&mut workers, 1, false, true, &mut search);
        assert_eq!(w.workplaces[&1].status, 1 << icon::BUILD_PAUSED);
    }
}
