//! The lives of citizens and their homes: needs, age, death, health, happiness, disease, homes,
//! partners and births. See [Citizens](https://tanepiper.github.io/unbanished/topics/citizens),
//! docs/CITIZEN-NEEDS-AND-HOUSEHOLDS.md and docs/CITIZEN-LIFE.md.
//!
//! The citizen description is kept as a table by offset, so the port reads the same values as
//! the original.
//!
//! Deviations: the entity references the original keeps (home, partner, guardian, dependents)
//! are citizen and entity indices; the dependents' ring is a vector in ring order.
//!
//! Original: `CitizenComponent` (vtable `0x1401689a0`), `ResidenceComponent` (vtable
//! `0x14016a6a0`); disease spread `FUN_140117940`.
mod update;

mod death;
mod health;
mod households;
mod services;
pub use health::outbreak_check;
pub use households::{update_home, warm_at};

// The CitizenDescription is at citizen +0x288.

use std::collections::BTreeMap;

use unbanished_formats::game::{CitizenDescription, Disease, ResidenceDescription};

use crate::behaviour::activities::TownView;
use crate::behaviour::agents::CitizenState;
use crate::pathfind::WALKABLE;
use crate::people::citizens::Citizen;
use crate::random::Random;
use crate::scheduler::{seconds, ticks};
use crate::work::stock::{RawMaterial, Storage};

/// `DAT_140178934`: the game's "minute" (most description times are in these units).
const MINUTE: f32 = 300.0;
/// `DAT_14017895c`.
const HOUR: f32 = 3600.0;

/// The CitizenDescription values, by offset.
#[derive(Debug, Clone, Default)]
pub struct LifeRules {
    fields: BTreeMap<u32, f32>,
    ints: BTreeMap<u32, i32>,
    // D+0x20; D+0x28; D+0x30
    /// `D`, `D`, `D`: the teacher's, priest's and doctor's professions (the workplaces whose
    /// attendance a citizen joins), as profession indices.
    pub services: [Option<usize>; 3],
}

// D+0x20/+0x28/+0x30
/// Which service an attendance search is for (`D/`).
pub mod service {
    /// A school.
    pub const SCHOOL: usize = 0;
    /// A chapel.
    pub const CHAPEL: usize = 1;
    /// A doctor.
    pub const DOCTOR: usize = 2;
}

impl LifeRules {
    /// The rules from the citizen description.
    pub fn new(d: &CitizenDescription) -> LifeRules {
        let mut r = LifeRules::default();
        // Constructor values (`FUN_14011a340`) of the fields the resource does not store:
        // illness length `+0x94` / `+0x98` and immunity `+0x9c`, in minutes.
        for (o, v) in [(0x94, 1.0), (0x98, 0.5), (0x9c, 12.0)] {
            r.fields.insert(o, v);
            r.ints.insert(o, v as i32);
        }
        for &o in d.fields.keys() {
            r.fields.insert(o, d.f(o));
            r.ints.insert(o, d.i(o));
        }
        r
    }

    /// Rules from description values by offset, for tests and tools.
    pub fn from_values(values: &[(u32, f32)]) -> LifeRules {
        let mut r = LifeRules::default();
        for &(o, v) in values {
            r.fields.insert(o, v);
            r.ints.insert(o, v as i32);
        }
        r
    }

    /// The float at `D + o`.
    pub fn f(&self, o: u32) -> f32 {
        self.fields.get(&o).copied().unwrap_or(0.0)
    }

    /// The integer at `D + o`.
    pub fn i(&self, o: u32) -> i32 {
        self.ints.get(&o).copied().unwrap_or(0)
    }

    /// The age at which a child becomes an adult.
    pub fn adult_age(&self) -> f32 {
        self.f(0x44)
    }

    /// `(D[0xd0] − D[0xcc]) · ½ + D[0xcc]`: hungry enough for the status icon and a health event.
    pub fn hunger_warning(&self) -> f32 {
        (self.f(0xd0) - self.f(0xcc)) * 0.5 + self.f(0xcc)
    }

    /// `(D[0xf0] − D[0xec]) · ¼ + D[0xec]`.
    pub fn cold_warning(&self) -> f32 {
        (self.f(0xf0) - self.f(0xec)) * 0.25 + self.f(0xec)
    }
}

/// The causes of death and the life events, as the event log's types.
///
/// Original: the reasons of `FUN_14011c2a0`.
pub mod reason {
    /// Starved.
    pub const STARVED: i32 = 0;
    /// Froze.
    pub const FROZE: i32 = 1;
    /// Died of old age.
    pub const OLD_AGE: i32 = 2;
    /// Died of disease.
    pub const DISEASE: i32 = 3;
    /// A child whose single guardian died.
    ///
    /// Original: `FUN_140119170`.
    pub const ORPHANED: i32 = 4;
    /// Killed by a tornado.
    ///
    /// Original: `FUN_140058d20`.
    pub const TORNADO: i32 = 5;
    /// Died in childbirth.
    pub const CHILDBIRTH: i32 = 6;
    // +0x1f8
    /// Citizen: an accident (fire, tornado) named by its description.
    pub const ACCIDENT: i32 = 7;
    /// Was born.
    pub const BORN: i32 = 8;
    /// Became an adult.
    pub const ADULT: i32 = 9;
    /// Became a student.
    pub const STUDENT: i32 = 10;
    /// A laborer took the dead citizen's profession.
    ///
    /// Original: `FUN_140119170`.
    pub const REPLACED: i32 = 0xf;
    /// No laborer took the dead citizen's profession.
    pub const NOT_REPLACED: i32 = 0x10;
}

/// An event the citizens' update asks the log for.
///
/// Original: `FUN_140118fc0`.
#[derive(Debug, Clone, PartialEq)]
pub struct LifeEvent {
    /// The event type.
    pub kind: i32,
    /// How important the event is.
    pub severity: i32,
    /// The citizen.
    pub citizen: usize,
    /// `@4`: a disease's or accident's name (string-table key and table).
    pub extra: Option<(Option<String>, String)>,
}

/// ResidenceComponent: the family slots and the fire.
#[derive(Debug, Clone, PartialEq)]
pub struct Home {
    /// The residence description.
    pub desc: ResidenceDescription,
    /// The site's StorageComponent.
    pub storage: usize,
    // +0x50
    /// The residents of each family slot, sorted as the original sorts them.
    ///
    /// Original: sorted by `FUN_14012f2f0`.
    pub families: Vec<Vec<usize>>,
    // +0x48
    /// Time the last firewood keeps burning.
    pub fuel: i64,
    /// The StatusIconComponent bits the residence sets: 6 no food, 7 no firewood.
    pub status: u32,
}

impl Home {
    /// A home with its description and its storage.
    pub fn new(desc: ResidenceDescription, storage: usize) -> Home {
        Home {
            desc,
            storage,
            families: vec![Vec::new(); desc.families.max(1) as usize],
            fuel: 0,
            status: 0,
        }
    }

    /// The number of residents.
    ///
    /// Original: `FUN_14012f4c0`.
    pub fn occupants(&self) -> usize {
        self.families.iter().map(Vec::len).sum()
    }

    // +0x24
    /// Family `slot` has at least residents.
    ///
    /// Original: `FUN_14012f9c0`.
    pub fn full(&self, slot: usize) -> bool {
        self.families
            .get(slot)
            .is_some_and(|f| f.len() as i32 >= self.desc.family_size)
    }

    /// Warm enough outside, or the fire is burning.
    ///
    /// Original: `FUN_140130100`.
    pub fn warm(&self, temperature: f32) -> bool {
        self.desc.warm_temperature <= temperature || self.fuel > 0
    }

    /// The number of families with residents.
    ///
    /// Original: counted by `FUN_14011abb0` and `FUN_14011acc0`.
    pub fn families_living(&self) -> i32 {
        self.families.iter().filter(|f| !f.is_empty()).count() as i32
    }
}

/// The units of a category in `storage` that nobody has reserved: food (bit 5) or firewood
/// (bit 6).
///
/// Original: `FUN_14012fea0` (food), `FUN_14012ffd0` (firewood).
pub fn stored(storage: &Storage, materials: &[RawMaterial], bit: u32) -> i32 {
    storage
        .items
        .iter()
        .filter(|i| materials[i.material].flags >> bit & 1 != 0)
        .map(|i| i.count - i.reserved)
        .sum()
}

/// `FUN_14012fbe0`'s sort (`FUN_14012f2f0`): citizens without a guardian first, then by sex.
pub(crate) fn family_order(citizens: &[Citizen], a: usize, b: usize) -> std::cmp::Ordering {
    let key = |c: usize| {
        let c = &citizens[c];
        (c.parent.is_some() as u32) << 12 | (c.male as u32) << 8
    };
    key(a).cmp(&key(b))
}

/// Everything one citizen's update touches.
pub struct People<'a, 'b> {
    /// The citizens.
    pub citizens: &'a mut Vec<Citizen>,
    /// The citizens' states.
    pub states: &'a mut Vec<CitizenState>,
    // +0xa0
    /// The living citizens, in activation order.
    pub registry: &'a mut Vec<usize>,
    /// The homes, by entity.
    pub homes: &'a mut BTreeMap<usize, Home>,
    /// The citizen description's rules.
    pub rules: &'a LifeRules,
    /// A view of the town.
    pub town: &'a TownView<'b>,
    // +0x1c..+0x27
    /// Map cells ([`crate::nature::seeding::Cells::layers`]).
    pub layers: &'a [[u8; 12]],
    /// The game's random numbers.
    pub rng: &'a mut Random,
    /// The environment's temperature.
    pub temperature: f32,
    /// The diseases.
    pub diseases: &'a [Disease],
    // +0x1a8
    /// The extra disease risk that visitors bring (0 until they are ported).
    pub town_disease: f32,
    /// The town's storages.
    pub storages: &'a mut [Storage],
    /// The raw materials.
    pub materials: &'a [RawMaterial],
    /// The character's action stack is empty (`stack_top + 1 < 1`).
    pub idle: &'a [bool],
    /// The log events asked for.
    pub events: &'a mut Vec<LifeEvent>,
    /// Citizens whose entity was queued for deletion, with the reason.
    ///
    /// Original: `FUN_1400c8370`.
    pub dying: &'a mut Vec<(usize, i32)>,
    /// Makes a child of `mother`: name, sex, appearance and lifespan, with their random draws.
    /// The caller of this closure adds it to `citizens`.
    ///
    /// Original: `FUN_14011c550`, called from `FUN_140117060`.
    pub newborn: &'a mut dyn FnMut(&mut Random, &Citizen, bool) -> Citizen,
    // (the flag is the child's sex, `true` male)
    /// Children born this update (new citizen indices).
    pub born: &'a mut Vec<usize>,
    // +0x60
    /// The AttendanceComponents, by entity.
    pub attendance: &'a mut BTreeMap<usize, crate::work::construction::Attendance>,
    /// The service buildings that people attend.
    pub attendance_list: &'a [usize],
    // +0x100; +0x20
    /// The workplaces and residences.
    pub workforce: &'a mut crate::work::professions::Workforce,
    /// The professions.
    pub professions: &'a crate::work::professions::Professions,
    /// The sites (their works' priorities, a cemetery's graves).
    pub sites: &'a BTreeMap<usize, crate::work::construction::Site>,
    // +0x40
    /// The town's cemeteries.
    pub cemeteries: &'a [usize],
    /// The tombstones standing, with the time each one stays.
    pub tombstones: &'a BTreeMap<usize, i64>,
    /// Burials decided this update (cemetery, grave cell); the tombstones follow.
    pub burials: &'a mut Vec<(usize, i32)>,
    /// The path-rank cache.
    pub sectors: &'a mut crate::sectors::SectorGraph,
    /// The ground's heights.
    pub terrain: &'a crate::terrain::HeightGrid<'b>,
    // Game +0x88
    /// The characters' tree (`Game`): a disease spreads to the citizens about the sick.
    pub tree: &'a crate::behaviour::character_tree::CharacterTree,
}

impl People<'_, '_> {
    fn adult(&self, c: usize) -> bool {
        self.citizens[c].age >= self.rules.adult_age()
    }

    fn cell(&self, c: usize) -> i32 {
        self.town.cell_of(self.citizens[c].position)
    }

    fn dying(&self, c: usize) -> bool {
        self.dying.iter().any(|&(d, _)| d == c)
    }

    /// `FUN_140118fc0`: a log entry naming the citizen and its role.
    fn report(
        &mut self,
        c: usize,
        kind: i32,
        severity: i32,
        extra: Option<(Option<String>, String)>,
    ) {
        self.events.push(LifeEvent {
            kind,
            severity,
            citizen: c,
            extra,
        });
    }

    /// The home's entrance is reachable from the citizen's cell.
    ///
    /// Original: `FUN_14011ada0`.
    pub fn reaches(&mut self, c: usize, entity: usize) -> bool {
        let cell = self.cell(c);
        self.town.can_reach(cell, entity, self.rng)
    }
}

// +0xc
/// Citizen message 2: eat from `storage` until the citizen is no longer hungry: one unit of food
/// per point of hunger. Each kind of food is remembered (for variety) and counts as used.
///
/// Original: `FUN_14011a7b0`, `FUN_14008e460`.
pub fn eat(
    s: &mut CitizenState,
    storage: &mut Storage,
    materials: &[RawMaterial],
    rng: &mut Random,
    counters: &mut crate::work::tracker::StockCounters,
) {
    while 0.0 < s.hunger {
        let Some(m) = storage.find_item(materials, 0, rng) else {
            return;
        };
        s.flags |= 0x10000;
        if !s.foods.contains(&m) {
            s.foods.push(m);
        }
        storage.remove(m, 1);
        let flags = materials[m].flags;
        if let Some(slot) = crate::work::produce::production_group(flags) {
            counters.used[slot] += 1;
        }
        if s.health_events & 8 == 0 {
            s.health_events = s.health_events & 0xfffffe8f | 8;
        }
        for (bit, event) in [(0, 0x10), (1, 0x20), (2, 0x40), (4, 0x100)] {
            if flags >> bit & 1 != 0 {
                s.health_events |= event;
            }
        }
        s.hunger -= 1.0;
        if s.hunger < 0.0 {
            s.hunger = 0.0;
            return;
        }
    }
}
