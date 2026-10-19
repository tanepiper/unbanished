//! Professions (`Game`), workplaces (WorkPlaceComponent, the town manager's workplace registry
//! `Game →`) and who takes a job when the player changes a profession's count. See docs/JOBS.md.
//!
//! The ProfessionUI spinners and the WorkPlaceUI buttons call the town manager directly in the
//! original, between simulation iterations; the port queues them as [`Command`]s that the
//! simulation applies before its next iteration, which is the same point in the game's Random
//! stream.
//!
//! Original: `FUN_14007b890`, `FUN_14008ccb0`.
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

/// A player action on professions, applied by the simulation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// ProfessionUI spinner `300 + profession` set to `count`.
    ///
    /// Original: `FUN_14007b890`.
    SetCount {
        /// The profession.
        profession: usize,
        /// The number of workers wanted.
        count: i32,
    },
    /// WorkPlaceUI `0x1450`: one more worker of the workplace's profession.
    ///
    /// Original: `FUN_1400918a0`.
    AddWorker {
        /// The workplace.
        entity: usize,
    },
    /// WorkPlaceUI `0x1451`: one fewer.
    ///
    /// Original: `FUN_140096650`.
    RemoveWorker {
        /// The workplace.
        entity: usize,
    },
    /// WorkPlaceUI `0x1452/0x1453`: most workers ± 1.
    ///
    /// Original: `FUN_14008cae0`, `FUN_140134fd0`.
    ChangeMaxWorkers {
        /// The workplace.
        entity: usize,
        /// The change: +1 or −1.
        delta: i32,
    },
    /// The pause button of a building's window (element 6000).
    ///
    /// Original: `FUN_1400b7370`, `FUN_1401350d0`.
    SetEnabled {
        /// The building.
        entity: usize,
        /// Whether it works.
        enabled: bool,
    },
    // slot +0x58
    /// CropUI's crop list: the field's crop; choosing the first one switches farming on.
    ///
    /// Original: `FUN_1400b7df0`.
    SetCrop {
        /// The field.
        entity: usize,
        /// The crop, or none.
        crop: Option<usize>,
    },
    /// The field's buttons: 0 plant now, 1 harvest now, 2 farming on/off.
    ///
    /// Original: `FUN_140120190`.
    FieldAction {
        /// The field.
        entity: usize,
        /// The button: 0, 1 or 2.
        action: i32,
    },
    // slot +0x48
    /// PastureUI's slider: the most animals the pasture keeps.
    ///
    /// Original: `FUN_1400ba2f0`.
    SetLimit {
        /// The pasture.
        entity: usize,
        /// The most animals.
        limit: i32,
    },
    /// Demolish a building (the Remove Buildings tool, or the button in its window).
    ///
    /// Original: `FUN_140109b40`, `FUN_140123470`.
    Demolish {
        /// The building.
        entity: usize,
    },
    /// DestroyUI `0x1771`: the demolition is called off.
    ///
    /// Original: `FUN_1401238d0`.
    CancelDemolition {
        /// The building.
        entity: usize,
    },
    /// RoadTool's apply: plan a road on a cell.
    ///
    /// Original: `FUN_14010a200`, `FUN_1400a1e30`.
    PlanRoad {
        /// The cell.
        cell: i32,
        /// The road kind.
        kind: u8,
        /// The cell type of the road.
        ty: u8,
        /// The raw material it is built from, if any.
        material: Option<usize>,
    },
    /// Remove Roads: take a planned road back; a built one gets a removal marker (`marker`, the
    /// tool's template).
    ///
    /// Original: `FUN_140108ab0`, `FUN_1400a1bd0`.
    ClearRoad {
        /// The cell.
        cell: i32,
        /// The removal marker's template.
        marker: usize,
    },
    // slot +0x10
    /// A StateMachine message: DestroyComponent's Remove Cancel (0 while torn down), DestroyUI
    /// `0x1772` (0: rebuild the rubble) and `0x1773` (1: clear it); ImmigrationUI `0x9858` (7: take
    /// the nomads in) and `0x9859` (8: turn them away).
    ///
    /// Original: `FUN_140123940`, `FUN_1401239a0`.
    StateMessage {
        /// The entity.
        entity: usize,
        /// The message.
        message: i32,
    },
    // slot +0x70
    /// ControlUI: the `kind` component's control `index`.
    ///
    /// Original: `FUN_140060150`.
    Control {
        /// The entity.
        entity: usize,
        /// The component kind.
        kind: u32,
        /// The control's index.
        index: i32,
    },
    // +0x50
    /// ConsumeProduceUI's product: ConsumeProduceComponent.
    ///
    /// Original: `FUN_14005fc90`.
    SetRecipe {
        /// The workplace.
        entity: usize,
        /// The product.
        recipe: i32,
    },
    // +0x60
    /// Upgrade a building to `template` (the upgrade button): it is demolished, and the new
    /// building takes its place.
    ///
    /// Original: `FUN_14008c160`, then `FUN_140123470`.
    Upgrade {
        /// The building.
        entity: usize,
        /// The template of the new building.
        template: usize,
    },
    /// TradeUI's trade button: sell (material, count) and buy (trade good, count).
    ///
    /// Original: `FUN_1400868f0`.
    Exchange {
        /// The trading post.
        entity: usize,
        /// What the town sells: `(raw material, count)`.
        sell: Vec<(usize, i32)>,
        /// What the town buys: `(trade good, count)`.
        buy: Vec<(usize, i32)>,
    },
    /// TradeUI's settings on the TradeComponent.
    Trade {
        /// The trading post.
        entity: usize,
        /// The setting.
        setting: crate::commerce::trade::TradeSetting,
    },
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

    /// Shuffle, then the first enabled workplace with a free place.
    ///
    /// Original: `FUN_1400925d0`.
    pub fn free_workplace(&mut self, profession: usize, rng: &mut Random) -> Option<usize> {
        self.shuffle_workplaces(profession, rng);
        let list = self.registry.get(&profession)?;
        list.iter().copied().find(|e| {
            self.workplaces
                .get(e)
                .is_some_and(|w| (w.workers.len() as i32) < w.max() && w.enabled)
        })
    }

    /// Make one laborer take `profession` at `wp`. It looks, in this order, for: a laborer who
    /// shares a home with someone of that profession, the laborer with a home nearest the
    /// workplace, the household with the most laborers, then the first laborer.
    ///
    /// Original: `FUN_1400918a0` (and the loop body of `FUN_14007b890`); the searches
    /// `FUN_1400920c0`, `FUN_140091ef0`, `FUN_140091ac0`, `FUN_140091980`.
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

    /// One citizen of `profession` becomes a laborer: the first without a workplace, else the first
    /// worker of the workplace with the most workers, else the first of that profession.
    ///
    /// Original: `FUN_140096650`.
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

    /// Adults per profession.
    ///
    /// Original: `FUN_14007ba70` (counts).
    pub fn counts(&self, workers: &[Worker], professions: usize) -> Vec<i32> {
        let mut out = vec![0; professions];
        for w in workers.iter().filter(|w| w.adult) {
            if let Some(n) = out.get_mut(w.profession) {
                *n += 1;
            }
        }
        out
    }

    /// The most workers of every enabled workplace of a profession.
    ///
    /// Original: `FUN_14007ba70` ("of N").
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

    /// Move the profession's count toward `count`, one citizen at a time.
    ///
    /// Original: `FUN_14007b890`.
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

    /// Set the most workers, then the first workers move to another workplace
    /// ([`Workforce::find_workplace`], or none) until they fit.
    ///
    /// Original: `FUN_140134fd0`.
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

    /// Accept workers or not; a disabled workplace's workers move to another
    /// ([`Workforce::find_workplace`], or none). Switched off it shows `BuildPaused` while being
    /// `building` (states 0, 1, 8), else `WorkDisabled`, and neither on; switched off it drops the
    /// limit, work and workers icons, on without workers of its own profession it shows
    /// `WorkersUnavailable`.
    ///
    /// Original: `FUN_1401350d0`.
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

    /// A workplace of citizen `c`'s profession, enabled, with room and reachable: for a profession
    /// that works anywhere, the one whose work comes first (by work priority), else the one with
    /// the most room, the first of equals.
    ///
    /// Original: `FUN_140115940`, `FUN_140115490`, `FUN_140115450`; the priority sort is
    /// `FUN_140115800`.
    pub fn find_workplace(&self, c: usize, p: usize, search: &mut dyn Search) -> Option<usize> {
        let list = self.registry.get(&p)?;
        let anywhere = search.anywhere(p);
        let cell = search.cell(c);
        let mut best = None;
        let mut most = 0;
        let mut candidates = Vec::new();
        for &e in list {
            let Some(w) = self.workplaces.get(&e) else {
                continue;
            };
            let free = w.max() - w.workers.len() as i32;
            if free <= 0 || !w.enabled || !search.reach(cell, e) {
                continue;
            }
            if anywhere {
                candidates.push((e, search.priority(e)));
            } else if most < free {
                most = free;
                best = Some(e);
            }
        }
        if anywhere {
            // `FUN_140030c00`: the CRT qsort by work priority (`FUN_140115800`); the first.
            crate::behaviour::ai::crt_qsort_less(&mut candidates, |a, b| a.1 < b.1);
            return candidates.first().map(|c| c.0);
        }
        best
    }

    /// Should citizen `c` look again? A profession that works anywhere when its workplace cannot be
    /// reached or another with room has work that comes first; the others when the reachable
    /// workplaces' room differs by more than one.
    ///
    /// Original: `FUN_140115eb0`.
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

    /// Every worker leaves.
    ///
    /// Original: `FUN_140134330`.
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
            // Handled by the crop field, orchard and pasture (`crate::production::farm`, `crate::production::pasture`).
            Command::SetCrop { .. }
            | Command::FieldAction { .. }
            | Command::SetLimit { .. }
            | Command::Demolish { .. }
            | Command::CancelDemolition { .. }
            | Command::StateMessage { .. }
            | Command::PlanRoad { .. }
            | Command::ClearRoad { .. }
            | Command::Control { .. }
            | Command::SetRecipe { .. }
            | Command::Upgrade { .. }
            | Command::Exchange { .. }
            | Command::Trade { .. } => {}
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
