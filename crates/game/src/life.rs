//! CitizenComponent (vtable `0x1401689a0`) and ResidenceComponent (vtable `0x14016a6a0`):
//! needs, age, death, health, happiness, disease, homes, partners and births. See
//! docs/CITIZEN-NEEDS-AND-HOUSEHOLDS.md and docs/CITIZEN-LIFE.md.
//!
//! The CitizenDescription (`D`, citizen `+0x288`) is kept as an offset table so the ported code
//! reads the same fields as the original.
//!
//! Deviations: the entity references the original keeps (home, partner, guardian, dependents)
//! are citizen and entity indices; the dependents' ring is a vector in ring order. Disease
//! spread (`FUN_140117940`) visits the citizens within the radius in registration order where
//! the original asks its spatial index.

use std::collections::BTreeMap;

use banished_formats::game::{CitizenDescription, Disease, ResidenceDescription};

use crate::activities::TownView;
use crate::agents::CitizenState;
use crate::citizens::Citizen;
use crate::pathfind::WALKABLE;
use crate::random::Random;
use crate::scheduler::{seconds, ticks};
use crate::stock::{RawMaterial, Storage};

/// `DAT_140178934`: the game's "minute" (most description times are in these units).
const MINUTE: f32 = 300.0;
/// `DAT_14017895c`.
const HOUR: f32 = 3600.0;

/// The CitizenDescription values, by offset.
#[derive(Debug, Clone, Default)]
pub struct LifeRules {
    fields: BTreeMap<u32, f32>,
    ints: BTreeMap<u32, i32>,
    /// `D+0x20`, `D+0x28`, `D+0x30`: the teacher's, priest's and doctor's professions (the
    /// workplaces whose attendance a citizen joins), as profession indices.
    pub services: [Option<usize>; 3],
}

/// Which service an attendance search is for (`D+0x20/+0x28/+0x30`).
pub mod service {
    pub const SCHOOL: usize = 0;
    pub const CHAPEL: usize = 1;
    pub const DOCTOR: usize = 2;
}

impl LifeRules {
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

    /// The float at `D + o`.
    pub fn f(&self, o: u32) -> f32 {
        self.fields.get(&o).copied().unwrap_or(0.0)
    }

    /// The integer at `D + o`.
    pub fn i(&self, o: u32) -> i32 {
        self.ints.get(&o).copied().unwrap_or(0)
    }

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

/// Death and life-event codes (`FUN_14011c2a0` reasons; the event log's types).
pub mod reason {
    pub const STARVED: i32 = 0;
    pub const FROZE: i32 = 1;
    pub const OLD_AGE: i32 = 2;
    pub const DISEASE: i32 = 3;
    /// A child whose single guardian died (`FUN_140119170`).
    pub const ORPHANED: i32 = 4;
    pub const CHILDBIRTH: i32 = 6;
    /// Citizen `+0x1f8`: an accident (fire, tornado) named by its description.
    pub const ACCIDENT: i32 = 7;
    pub const BORN: i32 = 8;
    pub const ADULT: i32 = 9;
    pub const STUDENT: i32 = 10;
    /// `FUN_140119170`: a laborer took the dead citizen's profession.
    pub const REPLACED: i32 = 0xf;
    pub const NOT_REPLACED: i32 = 0x10;
}

/// An event the citizens' update asks the log for (`FUN_140118fc0`).
#[derive(Debug, Clone, PartialEq)]
pub struct LifeEvent {
    pub kind: i32,
    pub severity: i32,
    pub citizen: usize,
    /// `@4`: a disease's or accident's name (string-table key and table).
    pub extra: Option<(Option<String>, String)>,
}

/// ResidenceComponent: the family slots and the fire.
#[derive(Debug, Clone, PartialEq)]
pub struct Home {
    pub desc: ResidenceDescription,
    /// The site's StorageComponent.
    pub storage: usize,
    /// `+0x50`: per family slot, its residents (sorted by `FUN_14012f2f0`).
    pub families: Vec<Vec<usize>>,
    /// `+0x48`: time the last firewood keeps burning.
    pub fuel: i64,
    /// The StatusIconComponent bits the residence sets: 6 no food, 7 no firewood.
    pub status: u32,
}

impl Home {
    pub fn new(desc: ResidenceDescription, storage: usize) -> Home {
        Home {
            desc,
            storage,
            families: vec![Vec::new(); desc.families.max(1) as usize],
            fuel: 0,
            status: 0,
        }
    }

    /// `FUN_14012f4c0`.
    pub fn occupants(&self) -> usize {
        self.families.iter().map(Vec::len).sum()
    }

    /// `FUN_14012f9c0`: family `slot` has at least `+0x24` residents.
    pub fn full(&self, slot: usize) -> bool {
        self.families
            .get(slot)
            .is_some_and(|f| f.len() as i32 >= self.desc.family_size)
    }

    /// `FUN_140130100`: warm enough outside, or the fire is burning.
    pub fn warm(&self, temperature: f32) -> bool {
        self.desc.warm_temperature <= temperature || self.fuel > 0
    }

    /// Families with residents (`FUN_14011abb0` / `FUN_14011acc0` count them).
    pub fn families_living(&self) -> i32 {
        self.families.iter().filter(|f| !f.is_empty()).count() as i32
    }
}

/// `FUN_14012fea0` (category bit 5, food) and `FUN_14012ffd0` (bit 6, firewood): unreserved
/// units of that category in `storage`.
pub fn stored(storage: &Storage, materials: &[RawMaterial], bit: u32) -> i32 {
    storage
        .items
        .iter()
        .filter(|i| materials[i.material].flags >> bit & 1 != 0)
        .map(|i| i.count - i.reserved)
        .sum()
}

/// `FUN_14012fbe0`'s sort (`FUN_14012f2f0`): citizens without a guardian first, then by sex.
fn family_order(citizens: &[Citizen], a: usize, b: usize) -> std::cmp::Ordering {
    let key = |c: usize| {
        let c = &citizens[c];
        (c.parent.is_some() as u32) << 12 | (c.male as u32) << 8
    };
    key(a).cmp(&key(b))
}

/// Everything one citizen's update touches.
pub struct People<'a, 'b> {
    pub citizens: &'a mut Vec<Citizen>,
    pub states: &'a mut Vec<CitizenState>,
    /// Town manager `+0xa0`: living citizens in activation order.
    pub registry: &'a mut Vec<usize>,
    pub homes: &'a mut BTreeMap<usize, Home>,
    pub rules: &'a LifeRules,
    pub town: &'a TownView<'b>,
    /// Map cells `+0x1c..+0x27` ([`crate::seeding::Cells::layers`]).
    pub layers: &'a [[u8; 12]],
    pub rng: &'a mut Random,
    pub temperature: f32,
    pub diseases: &'a [Disease],
    /// Town manager `+0x1a8`: extra disease risk brought by visitors (0 until they are ported).
    pub town_disease: f32,
    pub storages: &'a mut [Storage],
    pub materials: &'a [RawMaterial],
    /// The character's action stack is empty (`stack_top + 1 < 1`).
    pub idle: &'a [bool],
    pub events: &'a mut Vec<LifeEvent>,
    /// Citizens whose entity was queued for deletion (`FUN_1400c8370`), with the reason.
    pub dying: &'a mut Vec<(usize, i32)>,
    /// `FUN_140117060`'s `FUN_14011c550` for a child of `mother` (name, sex, appearance and
    /// lifespan draws); pushed onto `citizens` by the caller of this closure.
    pub newborn: &'a mut dyn FnMut(&mut Random, &Citizen, bool) -> Citizen,
    // (the flag is the child's sex, `true` male)
    /// Children born this update (new citizen indices).
    pub born: &'a mut Vec<usize>,
    /// AttendanceComponents and their registry (town manager `+0x60`).
    pub attendance: &'a mut BTreeMap<usize, crate::construction::Attendance>,
    pub attendance_list: &'a [usize],
    /// The workplaces and residences (town manager `+0x20`, `+0x100`).
    pub workforce: &'a mut crate::professions::Workforce,
    pub professions: &'a crate::professions::Professions,
    /// The sites (their works' priorities, a cemetery's graves).
    pub sites: &'a BTreeMap<usize, crate::construction::Site>,
    /// The town's cemeteries (`+0x40`) and the tombstones standing.
    pub cemeteries: &'a [usize],
    pub tombstones: &'a BTreeMap<usize, i64>,
    /// Burials decided this update (cemetery, grave cell); the tombstones follow.
    pub burials: &'a mut Vec<(usize, i32)>,
    /// The path-rank cache.
    pub sectors: &'a mut crate::sectors::SectorGraph,
    pub terrain: &'a crate::terrain::HeightGrid<'b>,
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

    /// `FUN_140118340`'s workplace search: without a workplace, or when `FUN_140115eb0` says
    /// to look again, `FUN_140115450` chooses one (or none) and `FUN_1401173b0` moves there.
    fn search_workplace(&mut self, c: usize) {
        let town = self.town;
        let citizens = &*self.citizens;
        let cell = |i: usize| town.cell_of(citizens[i].position);
        let professions = self.professions;
        let anywhere = |p: usize| professions.works_anywhere(p);
        let sites = self.sites;
        let priority = |e: usize| {
            sites
                .get(&e)
                .and_then(|s| s.work_priority)
                .unwrap_or(i32::MAX)
        };
        let mut search = crate::activities::TownSearch {
            town,
            cell: &cell,
            anywhere: &anywhere,
            priority: &priority,
            rng: &mut *self.rng,
        };
        let (p, current) = (self.states[c].profession, self.states[c].workplace);
        if current.is_none() || self.workforce.should_move(c, p, current, &mut search) {
            let wp = self.workforce.find_workplace(c, p, &mut search);
            if wp != current {
                self.workforce.move_worker(c, current, wp);
                self.states[c].workplace = wp;
            }
        }
    }

    /// `FUN_14011ada0`: the home's entrance is reachable from the citizen's cell.
    pub fn reaches(&mut self, c: usize, entity: usize) -> bool {
        let cell = self.cell(c);
        self.town.can_reach(cell, entity, self.rng)
    }

    /// `FUN_14011b8a0`: move citizen `c` (and its dependents) to `home` family `slot`.
    pub fn set_home(&mut self, c: usize, home: Option<usize>, slot: i32) {
        let s = &self.states[c];
        if s.home == home && s.home_slot == slot {
            return;
        }
        if let Some(old) = s.home {
            let old_slot = s.home_slot;
            self.remove_resident(old, old_slot, c);
        }
        self.states[c].home = home;
        self.states[c].home_slot = slot;
        if let Some(new) = home {
            self.add_resident(new, slot, c);
        }
        for d in self.states[c].dependents.clone() {
            self.set_home(d, home, slot);
        }
    }

    /// `FUN_14012fbe0`: append and sort the family; the first resident lights the house.
    fn add_resident(&mut self, home: usize, slot: i32, c: usize) {
        let citizens = &*self.citizens;
        if let Some(h) = self.homes.get_mut(&home) {
            if let Some(f) = h.families.get_mut(slot.max(0) as usize) {
                f.push(c);
                f.sort_by(|&a, &b| family_order(citizens, a, b));
            }
        }
    }

    /// `FUN_14012fa20`: remove; the last resident leaving clears the status icons 6 and 7.
    fn remove_resident(&mut self, home: usize, slot: i32, c: usize) {
        if let Some(h) = self.homes.get_mut(&home) {
            if let Some(f) = h.families.get_mut(slot.max(0) as usize) {
                if let Some(i) = f.iter().position(|&x| x == c) {
                    f.remove(i);
                }
            }
            if h.occupants() == 0 {
                crate::status::set_icon(&mut h.status, 6, false, false);
                crate::status::set_icon(&mut h.status, 7, false, false);
            }
        }
    }

    /// `FUN_140117470`: make `guardian` the guardian of `c`.
    pub fn set_guardian(&mut self, c: usize, guardian: Option<usize>) {
        if self.citizens[c].parent == guardian {
            return;
        }
        if let Some(g) = self.citizens[c].parent {
            self.remove_dependent(g, c);
        }
        if let Some(g) = guardian {
            self.add_dependent(g, c);
        }
    }

    /// `FUN_14011bd70`: join `g`'s ring (at its end) and its home.
    fn add_dependent(&mut self, g: usize, c: usize) {
        self.states[g].dependents.push(c);
        self.citizens[c].parent = Some(g);
        let (home, slot) = (self.states[g].home, self.states[g].home_slot);
        self.set_home(c, home, slot);
    }

    /// `FUN_140117510`: leave `g`'s ring; homeless and without a guardian.
    fn remove_dependent(&mut self, g: usize, c: usize) {
        self.states[g].dependents.retain(|&d| d != c);
        self.set_home(c, None, -1);
        self.citizens[c].parent = None;
    }

    /// The path-rank distance (`FUN_140008830`).
    fn rank(&mut self, a: i32, b: i32) -> i32 {
        let (size, terrain) = (self.town.size, self.terrain);
        let ground = move |cell: i32| {
            let n = size as i32;
            let h = (n >> 1) as f32 - 0.5;
            let (x, y) = ((cell % n) as f32 - h, (cell / n) as f32 - h);
            let mut normal = [0.0; 3];
            [x, y, terrain.ground(x, y, &mut normal)]
        };
        let mut g = crate::activities::StorageGeometry {
            town: self.town,
            sectors: self.sectors,
            ground: &ground,
        };
        crate::stock::StorageSearchMap::rank(&mut g, a, b)
    }

    /// `FUN_140044a50`: room, a worker, and the building reachable from the citizen.
    fn accepts(&self, e: usize, c: usize) -> bool {
        let Some(a) = self.attendance.get(&e) else {
            return false;
        };
        if a.attendees.len() as i32 >= a.capacity {
            return false;
        }
        if self
            .workforce
            .workplaces
            .get(&e)
            .is_none_or(|w| w.workers.is_empty())
        {
            return false;
        }
        let to = self.town.point_cell(e);
        to >= 0 && self.town.reachable(WALKABLE, self.cell(c), to)
    }

    /// `FUN_140116e00`: the accepting attendance of `service` nearest the home (path rank
    /// between entrances; any one without a home), skipping `exclude`.
    fn find_service(&mut self, c: usize, service: usize, exclude: Option<usize>) -> Option<usize> {
        let profession = self.rules.services[service]?;
        let home = self.states[c]
            .home
            .map_or(-1, |h| self.town.entrance(h, self.rng));
        let mut best = -1;
        let mut found = None;
        for &e in self.attendance_list {
            // The workplace's description profession (`FUN_140116e00` compares `+0x48`).
            if Some(e) == exclude
                || self.workforce.workplaces.get(&e).and_then(|w| w.profession) != Some(profession)
                || !self.accepts(e, c)
            {
                continue;
            }
            let entrance = self.town.entrance(e, self.rng);
            let d = if home >= 0 {
                self.rank(entrance, home)
            } else {
                1
            };
            if best < 0 || d < best {
                best = d;
                found = Some(e);
            }
        }
        found
    }

    /// `FUN_140116d10`, `FUN_140116bc0`, `FUN_140116a60`: join the nearest school, chapel or
    /// doctor; the school also sets `+0x220`.
    pub fn join_service(&mut self, c: usize, service: usize, exclude: Option<usize>) -> bool {
        let Some(e) = self.find_service(c, service, exclude) else {
            return false;
        };
        if let Some(a) = self.attendance.get_mut(&e) {
            a.attendees.push(c);
        }
        let s = &mut self.states[c];
        match service {
            service::SCHOOL => {
                s.school = Some(e);
                s.schooled = true;
            }
            service::CHAPEL => s.priest = Some(e),
            _ => s.doctor = Some(e),
        }
        true
    }

    /// `FUN_14011af20` / `FUN_140116b20` / `FUN_14011ae80`: leave; the school can be swapped
    /// for another at once (`again`).
    pub fn leave_service(&mut self, c: usize, service: usize, again: bool) {
        let slot = match service {
            service::SCHOOL => self.states[c].school.take(),
            service::CHAPEL => self.states[c].priest.take(),
            _ => self.states[c].doctor.take(),
        };
        let Some(e) = slot else { return };
        if let Some(a) = self.attendance.get_mut(&e) {
            a.attendees.retain(|&x| x != c);
        }
        if again && service == service::SCHOOL && !self.join_service(c, service, None) {
            self.states[c].school = None;
        }
    }

    /// Citizen message 4 (`FUN_140116c70`): a lesson; at `D[0x118]` lessons, or after
    /// `D[0x11c]` hours at school, educated (flag `0x400`), out of school and an adult (9).
    pub fn lesson(&mut self, c: usize) {
        let r = self.rules;
        let s = &mut self.states[c];
        self.citizens[c].education += 1;
        if self.citizens[c].education < r.i(0x118) && seconds(s.school_time) <= r.f(0x11c) * HOUR {
            return;
        }
        s.flags |= 0x400;
        self.citizens[c].education = r.i(0x118);
        self.citizens[c].educated = true;
        self.leave_service(c, service::SCHOOL, false);
        self.report(c, reason::ADULT, 0, None);
    }

    /// Citizen message 5 (`FUN_140116a10`): the doctor shortens the illness by `D[0x12c]`
    /// minutes.
    pub fn treated(&mut self, c: usize) {
        if self.states[c].disease.is_some() {
            self.states[c].disease_left -= ticks(self.rules.f(0x12c) * MINUTE);
        }
    }

    /// `FUN_14011c2a0`: queue the citizen's death (`reason`) and tell the family. Buried in a
    /// cemetery (`FUN_14008ec10` from the home's entrance, else where the citizen stands), the
    /// family's bit is `0x40`; otherwise `2` for old age, else `0x80`.
    /// Not ported: the debug flag that stops deaths (`Game +0xd0` bit 8).
    pub fn die(&mut self, c: usize, reason: i32, extra: Option<(Option<String>, String)>) -> bool {
        if self.dying(c) {
            return true;
        }
        let from = match self.states[c].home {
            Some(h) => self.town.point_cell(h),
            None => self.cell(c),
        };
        let bit = match self.find_cemetery(from) {
            Some(cemetery) => {
                if let Some(slot) = self.free_grave(cemetery) {
                    let cell = self.sites[&cemetery].graves[slot];
                    self.burials.push((cemetery, cell));
                }
                0x40
            }
            None if reason == reason::OLD_AGE => 2,
            None => 0x80,
        };
        if let Some(g) = self.citizens[c].parent {
            self.states[g].flags |= bit;
        }
        if let Some(s) = self.citizens[c].spouse {
            self.states[s].flags |= bit;
            for d in self.states[s].dependents.clone() {
                self.states[d].flags |= bit;
            }
        }
        for d in self.states[c].dependents.clone() {
            self.states[d].flags |= bit;
        }
        self.report(c, reason, 1, extra);
        self.dying.push((c, reason));
        true
    }

    /// `FUN_14008ec10`: the cemetery nearest `cell` by path rank (or `n + distance`) whose
    /// use point can be reached and which has a free grave.
    fn find_cemetery(&mut self, cell: i32) -> Option<usize> {
        let mut best = None;
        let mut nearest = i32::MAX;
        let n = self.town.size as i32;
        for &e in self.cemeteries {
            let to = self.town.point_cell(e);
            let mut rank = self.rank(cell, to);
            if rank < 0 {
                let (dx, dy) = ((cell % n - to % n) as f32, (cell / n - to / n) as f32);
                rank = n + (dx * dx + dy * dy).sqrt() as i32;
            }
            if rank < nearest
                && self.town.reachable(WALKABLE, cell, to)
                && self.free_grave(e).is_some()
            {
                best = Some(e);
                nearest = rank;
            }
        }
        best
    }

    /// `FUN_1400475c0`: from a random grave on, the first without a tombstone.
    fn free_grave(&mut self, cemetery: usize) -> Option<usize> {
        let graves = &self.sites.get(&cemetery)?.graves;
        let count = graves.len();
        let start = self.rng.rand_below(count as u32) as usize;
        (0..count).map(|i| (start + i) % count).find(|&i| {
            let cell = graves[i];
            let taken = self.town.objects[cell as usize]
                .is_some_and(|o| self.tombstones.contains_key(&o))
                || self.burials.iter().any(|&(_, b)| b == cell);
            !taken
        })
    }

    /// The family part of `FUN_140119170` (CitizenComponent removal): leave school, the
    /// doctor, the chapel, the home and the guardian; a widow(er) takes the dependents, else
    /// children die (orphaned) and older dependents leave; the partner is single again.
    pub fn bury(&mut self, c: usize) {
        self.leave_service(c, service::SCHOOL, false);
        self.leave_service(c, service::DOCTOR, false);
        self.leave_service(c, service::CHAPEL, false);
        self.set_home(c, None, -1);
        if let Some(g) = self.citizens[c].parent {
            self.remove_dependent(g, c);
        }
        match self.citizens[c].spouse {
            None => {
                let adult = self.rules.adult_age();
                for d in self.states[c].dependents.clone() {
                    if self.citizens[d].age <= adult {
                        self.die(d, reason::ORPHANED, None);
                    } else {
                        self.set_guardian(d, None);
                        self.set_home(d, None, -1);
                        self.leave_service(d, service::SCHOOL, false);
                    }
                }
            }
            Some(p) => {
                for d in self.states[c].dependents.clone() {
                    self.set_guardian(d, Some(p));
                }
            }
        }
        if let Some(p) = self.citizens[c].spouse.take() {
            self.citizens[p].spouse = None;
        }
        self.registry.retain(|&x| x != c);
    }

    /// `FUN_140118340`: one scheduled update of citizen `c` over `dt`.
    pub fn update(&mut self, c: usize, dt: i64) {
        let r = self.rules;
        let secs = seconds(dt);
        let adult_age = r.adult_age();
        let was_child = self.citizens[c].age < adult_age || self.states[c].school.is_some();
        let age = secs * r.f(0x40) + self.citizens[c].age;
        self.citizens[c].age = age;
        {
            let s = &mut self.states[c];
            s.health_timer += dt;
            s.happiness_timer += dt;
            s.clothing_remaining -= dt;
            if s.school.is_some() {
                s.school_time += dt;
            }
            s.disease_clock += dt;
            s.accident_timer += dt;
            if s.clothing_remaining < 0 {
                s.clothing = 0;
                s.clothing_remaining = 0;
            }
        }
        // Coming of age: a school place (code 10) or adulthood (code 9).
        if was_child && adult_age <= age && self.states[c].school.is_none() {
            let student = !self.states[c].schooled && self.join_service(c, service::SCHOOL, None);
            self.report(
                c,
                if student {
                    reason::STUDENT
                } else {
                    reason::ADULT
                },
                0,
                None,
            );
        }
        let s = &mut self.states[c];
        if s.sick && s.doctor.is_some() {
            s.hunger = 0.0;
            s.cold = 0.0;
        }
        s.hunger += secs * r.f(0xc0) / HOUR;
        if r.f(0xf4) <= self.temperature {
            s.cold = 0.0;
        } else {
            let rate = r.f(0xdc + 4 * s.clothing.clamp(0, 2) as u32);
            s.cold += secs * rate / MINUTE;
        }
        // An accident at work (`+0x1f8`, `FUN_14011bb20`): code 7 with the profession's
        // death message.
        if let Some(p) = self.states[c].accident {
            let extra = self
                .professions
                .list
                .get(p)
                .map(|p| (p.string_table.clone(), p.death.clone()));
            if self.die(c, reason::ACCIDENT, extra) {
                return;
            }
        }
        let s = &mut self.states[c];
        if r.f(0xd0) <= s.hunger {
            s.hunger = r.f(0xd0);
            if self.idle[c] && self.die(c, reason::STARVED, None) {
                return;
            }
        }
        let s = &mut self.states[c];
        if r.f(0xf0) <= s.cold {
            s.cold = r.f(0xf0);
            if self.idle[c] && self.die(c, reason::FROZE, None) {
                return;
            }
        }
        if self.citizens[c].lifespan <= self.citizens[c].age && self.die(c, reason::OLD_AGE, None) {
            return;
        }
        if !self.disease(c, dt) {
            let extra = self.states[c]
                .disease
                .and_then(|d| self.diseases.get(d))
                .map(|d| (d.table.clone(), d.name.clone()));
            if self.die(c, reason::DISEASE, extra) {
                return;
            }
        }
        if self.states[c].disease.is_some() {
            self.states[c].flags |= 1;
            if self.states[c].doctor.is_none() {
                self.join_service(c, service::DOCTOR, None);
            }
        }
        if self.states[c].priest.is_none() {
            self.join_service(c, service::CHAPEL, None);
        }
        let warn_hunger = r.hunger_warning();
        let warn_cold = r.cold_warning();
        {
            let s = &mut self.states[c];
            crate::status::set_icon(&mut s.status, 0, s.home.is_none(), false);
            crate::status::set_icon(&mut s.status, 1, warn_hunger < s.hunger, false);
            crate::status::set_icon(&mut s.status, 2, warn_cold < s.cold, false);
            crate::status::set_icon(&mut s.status, 5, s.disease.is_some(), false);
            if warn_hunger < s.hunger {
                s.health_events |= 1;
                s.flags |= 1;
            }
            if warn_cold < s.cold {
                s.health_events |= 2;
                s.flags |= 1;
            }
        }
        let interval = ticks(r.f(0x78) * MINUTE);
        while interval > 0 && interval < self.states[c].health_timer {
            self.states[c].health_timer -= interval;
            self.health(c);
        }
        let interval = ticks(r.f(0xa0) * MINUTE);
        while interval > 0 && interval < self.states[c].happiness_timer {
            self.states[c].happiness_timer -= interval;
            self.happiness(c);
        }
        // A home that cannot be reached is given up.
        if self.idle[c] {
            if let Some(home) = self.states[c].home {
                if !self.reaches(c, home) {
                    self.set_home(c, None, -1);
                }
            }
        }
        let child = self.citizens[c].age < adult_age || self.states[c].school.is_some();
        // A child without a home follows its guardian's.
        if child && self.idle[c] && self.states[c].home.is_none() {
            if let Some(g) = self.citizens[c].parent {
                if let Some(home) = self.states[g].home {
                    if self.reaches(c, home) {
                        let slot = self.states[g].home_slot;
                        self.set_home(c, Some(home), slot);
                    }
                }
            }
        }
        if !child && self.idle[c] {
            self.search_workplace(c);
            if self.states[c].home.is_none() || self.citizens[c].parent.is_some() {
                if self.states[c].home.is_none() {
                    self.states[c].flags |= 0x20;
                }
                let homeless = self.states[c].home.is_none();
                if let Some((home, slot)) = self.find_home(c, homeless) {
                    if self.citizens[c].parent.is_some() {
                        self.states[c].flags |= 0x2000;
                    }
                    self.set_guardian(c, None);
                    self.set_home(c, Some(home), slot);
                    if let Some(p) = self.citizens[c].spouse {
                        self.set_home(p, Some(home), slot);
                    }
                }
            }
            // A family in a shared residence looks for a house of its own.
            if let Some(home) = self.states[c].home {
                if self.homes.get(&home).is_some_and(|h| h.families.len() > 1) {
                    if let Some((home, slot)) = self.find_home(c, false) {
                        self.set_home(c, Some(home), slot);
                        if let Some(p) = self.citizens[c].spouse {
                            self.set_home(p, Some(home), slot);
                        }
                    }
                }
            }
            // Marriage.
            if self.citizens[c].parent.is_none()
                && self.states[c].home.is_some()
                && self.citizens[c].spouse.is_none()
                && r.f(0x48) <= self.citizens[c].age
            {
                if let Some(p) = self.find_partner(c) {
                    self.citizens[c].spouse = Some(p);
                    self.states[c].flags |= 0x200;
                    self.states[p].flags |= 0x200;
                    self.set_guardian(p, None);
                    self.citizens[p].spouse = Some(c);
                    let (home, slot) = (self.states[c].home, self.states[c].home_slot);
                    self.set_home(p, home, slot);
                }
            }
        }
        self.pregnancy(c, secs);
    }

    /// The birth part of `FUN_140118340`.
    fn pregnancy(&mut self, c: usize, secs: f32) {
        let r = self.rules;
        if self.citizens[c].male
            || self.citizens[c].age < r.adult_age()
            || self.states[c].school.is_some()
        {
            return;
        }
        let s = &mut self.states[c];
        if (r.hunger_warning() < s.hunger || r.cold_warning() < s.cold) && 0.0 < s.reproduction {
            s.reproduction = 0.0;
        }
        let t = s.reproduction;
        if t <= 0.0 {
            if t < 0.0 {
                s.reproduction = t + secs;
                if 0.0 <= t + secs {
                    s.reproduction = 0.0;
                }
            }
        } else {
            s.reproduction = t - secs;
            if t - secs <= 0.0 {
                let health = s.health;
                let threshold = (health / r.f(0x70)) * (r.f(0x64) - r.f(0x68)) + r.f(0x68);
                if threshold <= self.rng.rand_float() {
                    self.give_birth(c);
                    let cooldown = self.rng.rand_float() * r.f(0x60) + r.f(0x5c);
                    self.states[c].reproduction = -cooldown;
                } else if self.die(c, reason::CHILDBIRTH, None) {
                    return;
                }
            }
        }
        let s = &self.states[c];
        if self.citizens[c].spouse.is_some() && s.home.is_some() && s.reproduction == 0.0 {
            let age = self.citizens[c].age;
            if r.f(0x50) <= age && age <= r.f(0x54) {
                let full = s
                    .home
                    .and_then(|h| self.homes.get(&h))
                    .is_some_and(|h| h.full(s.home_slot.max(0) as usize));
                if !full {
                    self.states[c].reproduction = r.f(0x58);
                }
            }
        }
    }

    /// `FUN_140117060`: a child, its sex chosen to even out the unmarried young.
    fn give_birth(&mut self, mother: usize) {
        let r = self.rules;
        // The unmarried no older than `D[0x4c]` and the boys among them: a boy is likelier the
        // fewer boys there are.
        let (mut young, mut boys) = (0, 0);
        for &i in self.registry.iter() {
            if self.citizens[i].spouse.is_none() && self.citizens[i].age <= r.f(0x4c) {
                if self.citizens[i].male {
                    boys += 1;
                }
                young += 1;
            }
        }
        let male = if young != 0 {
            let x = self.rng.rand_float();
            boys as f32 / young as f32 <= x
        } else {
            self.rng.rand_below(2) == 1
        };
        let mut child = (self.newborn)(self.rng, &self.citizens[mother], male);
        child.position = self.citizens[mother].position;
        child.age = 0.0;
        let id = self.citizens.len();
        self.citizens.push(child);
        self.states.push(CitizenState::newborn(r));
        self.add_dependent(mother, id);
        self.registry.push(id);
        self.born.push(id);
        self.report(id, reason::BORN, 0, None);
        self.states[mother].flags |= 0x100;
        if let Some(f) = self.citizens[mother].spouse {
            self.states[f].flags |= 0x100;
        }
    }

    /// `FUN_140115310`: the first registered single of the other sex old enough to marry and
    /// close enough in age.
    fn find_partner(&self, c: usize) -> Option<usize> {
        let r = self.rules;
        let me = &self.citizens[c];
        self.registry.iter().copied().find(|&i| {
            let o = &self.citizens[i];
            o.male != me.male
                && o.spouse.is_none()
                && r.f(0x48) < o.age
                && (o.age - me.age).abs() <= r.f(0x4c)
                && !self.dying(i)
        })
    }

    /// `FUN_140116340`: a home for `c`. A partner's single-family home, unless an adult in a
    /// shared home or homeless waits for one; else the first free family slot of a
    /// single-family residence reachable from `c`, else (with `shared`) the first shared one.
    fn find_home(&mut self, c: usize, shared: bool) -> Option<(usize, i32)> {
        if let Some(p) = self.citizens[c].spouse {
            if let Some(home) = self.states[p].home {
                if self.reaches(c, home)
                    && self.homes.get(&home).is_some_and(|h| h.families.len() < 2)
                {
                    return Some((home, self.states[p].home_slot));
                }
            }
        }
        if let Some(mine) = self.states[c].home {
            // Moving out of a shared residence waits while another adult is homeless, or (when
            // this citizen's home is single-family) lives in a shared one.
            let single = self.homes.get(&mine).is_some_and(|h| h.families.len() == 1);
            let adult = self.rules.adult_age();
            for &o in self.registry.iter() {
                if o == c || self.citizens[o].age < adult || self.states[o].school.is_some() {
                    continue;
                }
                let Some(h) = self.states[o].home else {
                    return None;
                };
                if single && self.homes.get(&h).is_some_and(|h| h.families.len() > 1) {
                    return None;
                }
            }
        }
        let cell = self.cell(c);
        let mut fallback = None;
        for e in self.workforce.residences.clone() {
            let Some(home) = self.homes.get(&e) else {
                continue;
            };
            if !shared && home.families.len() >= 2 {
                continue;
            }
            for (slot, family) in home.families.iter().enumerate() {
                if !family.is_empty() {
                    continue;
                }
                let entrance = self.town.point_cell(e);
                if entrance < 0 || !self.town.reachable(WALKABLE, cell, entrance) {
                    continue;
                }
                if home.families.len() == 1 {
                    return Some((e, slot as i32));
                }
                if fallback.is_none() {
                    fallback = Some((e, slot as i32));
                }
            }
        }
        fallback
    }

    /// `FUN_140117e60`: health from the last interval's events.
    fn health(&mut self, c: usize) {
        let r = self.rules;
        let adult = self.adult(c) && self.states[c].school.is_none();
        let s = &mut self.states[c];
        let e = s.health_events;
        let mut n = (e >> 8 & 1) + (e >> 6 & 1) + (e >> 5 & 1) + (e >> 4 & 1);
        if e & 0x200 != 0 {
            n += 1;
        }
        if e & 8 != 0 && e & 4 != 0 && 3 < n {
            s.health += 0.5;
        }
        if e & 8 != 0 && e & 0x200 == 0 && n < 3 {
            s.health -= 1.0 - n as f32 / 3.0;
        }
        if e & 1 != 0 {
            s.health -= 1.0;
        }
        if e & 2 != 0 {
            s.health -= 1.0;
        }
        if e & 4 == 0 && adult {
            s.health -= 0.25;
        }
        if s.health < 0.0 {
            s.health = 0.0;
        }
        s.health = s.health.min(r.f(0x70));
        s.health_events = 0;
    }

    /// `FUN_140117fb0`: happiness from events, needs, food variety, the home's surroundings
    /// and the chapel.
    fn happiness(&mut self, c: usize) {
        let r = self.rules;
        let home_bonus = self.states[c].home.map(|h| {
            // The home entrance cell's layers 0..=4 that are nonzero, less layer 5 (the
            // HappinessComponent radii, `FUN_1400a4e60`), at least 0.
            let cell = self.town.entrance(h, self.rng);
            let l = usize::try_from(cell)
                .ok()
                .and_then(|i| self.layers.get(i))
                .copied()
                .unwrap_or_default();
            let v = (l[..5].iter().filter(|&&b| b != 0).count() as i32 - l[5] as i32).max(0);
            if self.homes.get(&h).is_some_and(|h| h.families.len() > 1) {
                v >> 2
            } else {
                v
            }
        });
        let s = &mut self.states[c];
        let f = s.flags;
        let mut h = s.happiness;
        if f & 2 != 0 {
            h -= 8.0;
        }
        if f & 0x40 != 0 {
            h -= 2.0;
        }
        if f & 0x80 != 0 {
            h -= 4.0;
        }
        if f & 8 != 0 {
            h -= 1.0;
        }
        if f & 0x10 != 0 {
            h -= 1.0;
        }
        for bit in [8, 9, 10, 11, 12, 13] {
            if f >> bit & 1 != 0 {
                h += 1.0;
            }
        }
        if f >> 0x12 & 1 != 0 {
            h += 0.5;
        }
        h = h.max(0.0).min(r.f(0xa4));
        let mut target = 0.0;
        if f & 4 == 0 {
            target += 1.0;
        }
        if f & 0x20 == 0 {
            target += 1.0;
        }
        if f & 1 == 0 {
            target += 1.0;
        }
        if f >> 0x11 & 1 != 0 {
            // Not traced: what counts citizen `+0x84` up; it stays 0 (the half bonus).
            target += if s.field_84 < r.i(0xb0) { 0.5 } else { 1.0 };
        }
        if f >> 0x10 & 1 != 0 {
            let foods = s.foods.len() as i32;
            if r.i(0xa8) < foods {
                if r.i(0xac) <= foods {
                    target += 1.0;
                }
            } else {
                target -= 1.0;
            }
        }
        if let Some(b) = home_bonus {
            target += b as f32;
        }
        if s.priest.is_some() {
            target += 1.0;
        }
        let target = target * 0.125 * r.f(0xa4);
        if h < target {
            h += 0.5;
        } else if target < h {
            // Through a double (`DAT_140178868` is 0.5 as a double).
            h = (h as f64 - 0.5) as f32;
        }
        s.happiness = h.max(0.0).min(r.f(0xa4));
    }

    /// `FUN_140117ca0`: false when the disease kills the citizen.
    fn disease(&mut self, c: usize, dt: i64) -> bool {
        let r = self.rules;
        match self.states[c].disease {
            None => {
                let s = &mut self.states[c];
                s.immunity = (s.immunity - dt).max(0);
                let weak = 1.0 - s.health / r.f(0x70);
                let a = (r.f(0x84) - r.f(0x7c)) * weak + r.f(0x7c);
                let b = (r.f(0x88) - r.f(0x80)) * weak + r.f(0x80);
                let a = (r.f(0x8c) - a) * self.town_disease + a;
                let b = (r.f(0x90) - b) * self.town_disease + b;
                let elapsed = s.disease_clock;
                s.disease_clock = 0;
                if b <= 0.0 {
                    return true;
                }
                let span = a * 12.0 * MINUTE * b;
                let x = self.rng.rand_float();
                if x < seconds(elapsed) / span {
                    self.outbreak(c);
                }
                true
            }
            Some(d) => {
                let s = &mut self.states[c];
                s.disease_left -= dt;
                s.spread -= dt;
                if s.spread < 1 {
                    self.spread(c);
                    let interval = self.diseases.get(d).map_or(0.0, |d| d.spread_interval);
                    self.states[c].spread = ticks(interval * MINUTE);
                }
                if self.states[c].disease_left < 1 {
                    return self.recover(c);
                }
                true
            }
        }
    }

    /// `FUN_140117af0`: unless someone is already sick, a disease picked by weight.
    fn outbreak(&mut self, c: usize) {
        if self
            .registry
            .iter()
            .any(|&i| self.states[i].disease.is_some())
        {
            return;
        }
        let mut total = 0.0;
        let cumulative: Vec<f32> = self
            .diseases
            .iter()
            .map(|d| {
                total += d.weight;
                total
            })
            .collect();
        let x = self.rng.rand_float() * total;
        let mut low = 0.0;
        for (i, &high) in cumulative.iter().enumerate() {
            if low <= x && x < high {
                self.infect(c, i);
                break;
            }
            low = high;
        }
    }

    /// `FUN_14011c060`: catch disease `d` (not while immune, nor a doctor at work).
    /// Not ported: the doctor exception (profession `+0x4d` with a workplace) and the
    /// AutoSlow request of the outbreak event.
    fn infect(&mut self, c: usize, d: usize) {
        let s = &self.states[c];
        if s.disease.is_some() || 0 < s.immunity {
            return;
        }
        let first = !self
            .registry
            .iter()
            .any(|&i| self.states[i].disease.is_some());
        if first {
            let extra = self
                .diseases
                .get(d)
                .map(|d| (d.table.clone(), d.name.clone()));
            self.report(c, 0xb, 3, extra);
        }
        let r = self.rules;
        let x = self.rng.rand_float_tol();
        let s = &mut self.states[c];
        s.disease = Some(d);
        s.sick = true;
        s.disease_left = ticks((x * r.f(0x98) + r.f(0x94)) * MINUTE);
        s.spread = ticks(self.diseases.get(d).map_or(0.0, |d| d.spread_interval) * MINUTE);
        self.join_service(c, service::DOCTOR, None);
    }

    /// `FUN_140117940`: perhaps infect one healthy citizen within the disease's radius.
    fn spread(&mut self, c: usize) {
        let Some(d) = self.states[c].disease else {
            return;
        };
        let Some(disease) = self.diseases.get(d).cloned() else {
            return;
        };
        let x = self.rng.rand_float();
        if disease.spread_chance < x {
            return;
        }
        let p = self.citizens[c].position;
        let r2 = disease.spread_radius * disease.spread_radius;
        let near: Vec<usize> = self
            .registry
            .iter()
            .copied()
            .filter(|&i| {
                let q = self.citizens[i].position;
                i != c && (q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2) <= r2
            })
            .collect();
        if let Some(&i) = near.iter().find(|&&i| self.states[i].disease.is_none()) {
            self.infect(i, d);
        }
    }

    /// `FUN_140117880`: when the illness ends, recover (immune for `D[0x9c]` minutes) or die.
    fn recover(&mut self, c: usize) -> bool {
        let r = self.rules;
        let x = self.rng.rand_float();
        let s = &self.states[c];
        let doctor = if s.doctor.is_some() { r.f(0x128) } else { 1.0 };
        let recovery = s
            .disease
            .and_then(|d| self.diseases.get(d))
            .map_or(1.0, |d| d.recovery);
        let lives = 1.0 / recovery / doctor < x;
        if lives {
            let s = &mut self.states[c];
            s.spread = 0;
            s.immunity = ticks(r.f(0x9c) * MINUTE);
            s.disease = None;
            s.sick = false;
        }
        // `FUN_14011ae80`: leave the doctor.
        self.leave_service(c, service::DOCTOR, false);
        lives
    }
}

/// Citizen message 0 (`FUN_1401176b0`): warm up at `home` if it is warm.
pub fn warm_at(s: &mut CitizenState, home: Option<&Home>, temperature: f32) {
    if home.is_some_and(|h| h.warm(temperature)) {
        s.cold = 0.0;
    }
}

/// Citizen message 2 (`FUN_14011a7b0`): eat from `storage` until no longer hungry: one unit of
/// food (`FUN_140094410` category 0) per point of hunger, remembering each kind for variety.
/// Not ported: the stock's consumption counter (`FUN_14008e460`).
pub fn eat(
    s: &mut CitizenState,
    storage: &mut Storage,
    materials: &[RawMaterial],
    rng: &mut Random,
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

/// `FUN_14012ee40`: a residence's update. The fire burns down; with residents, while it is
/// colder than the house's warmth, one unit of firewood keeps it burning for `+0x2c` seconds.
/// The food and firewood status icons clear when there is some.
/// Not ported: the ClearStorage work of an empty house that still holds goods, and the
/// stock's consumption counter.
pub fn update_home(
    home: &mut Home,
    storages: &mut [Storage],
    materials: &[RawMaterial],
    temperature: f32,
    dt: i64,
) {
    home.fuel -= dt;
    let storage = &mut storages[home.storage];
    if 0 < stored(storage, materials, 5) {
        crate::status::set_icon(&mut home.status, 6, false, false);
    }
    if 0 < stored(storage, materials, 6) {
        crate::status::set_icon(&mut home.status, 7, false, false);
    }
    if temperature < home.desc.warm_temperature && home.fuel < 0 && !home.families.is_empty() {
        let fuel = storage
            .items
            .iter()
            .find(|i| materials[i.material].flags >> 6 & 1 != 0 && 0 < i.count - i.reserved)
            .map(|i| i.material);
        if let Some(m) = fuel {
            home.fuel = ticks(home.desc.fuel_seconds);
            storage.remove(m, 1);
        }
    }
}
