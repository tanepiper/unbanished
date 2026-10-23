//! Households: homes and their residents, guardians and dependents, finding a home and a partner, pregnancy and births, and a home's warmth and stock.

use super::*;

/// Citizen message 0: warm up at `home` if it is warm.
///
/// Original: `FUN_1401176b0`.
pub fn warm_at(s: &mut CitizenState, home: Option<&Home>, temperature: f32) {
    if home.is_some_and(|h| h.warm(temperature)) {
        s.cold = 0.0;
    }
}

// +0xc; +0x2c
/// A residence's update. The fire burns down; with residents, while it is colder than the house's
/// warmth, one unit of firewood keeps it burning for seconds. The food and firewood status icons
/// clear when there is some. The ClearStorage work comes first
/// ([`crate::town::Sim::residence_clear_storage`]). The firewood burned counts as used in the stock
/// manager.
///
/// Original: `FUN_14012ee40`.
pub fn update_home(
    home: &mut Home,
    storages: &mut [Storage],
    materials: &[RawMaterial],
    temperature: f32,
    dt: i64,
    counters: &mut crate::work::tracker::StockCounters,
) {
    home.fuel -= dt;
    let storage = &mut storages[home.storage];
    if 0 < stored(storage, materials, 5) {
        crate::people::status::set_icon(&mut home.status, 6, false, false);
    }
    if 0 < stored(storage, materials, 6) {
        crate::people::status::set_icon(&mut home.status, 7, false, false);
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
            if let Some(slot) = crate::work::produce::production_group(materials[m].flags) {
                counters.used[slot] += 1;
            }
        }
    }
}

impl People<'_, '_> {
    /// Move citizen `c` (and its dependents) to `home` family `slot`.
    ///
    /// Original: `FUN_14011b8a0`.
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
    pub(super) fn add_resident(&mut self, home: usize, slot: i32, c: usize) {
        let citizens = &*self.citizens;
        if let Some(h) = self.homes.get_mut(&home) {
            if let Some(f) = h.families.get_mut(slot.max(0) as usize) {
                f.push(c);
                f.sort_by(|&a, &b| family_order(citizens, a, b));
            }
        }
    }

    /// `FUN_14012fa20`: remove; the last resident leaving clears the status icons 6 and 7.
    pub(super) fn remove_resident(&mut self, home: usize, slot: i32, c: usize) {
        if let Some(h) = self.homes.get_mut(&home) {
            if let Some(f) = h.families.get_mut(slot.max(0) as usize) {
                if let Some(i) = f.iter().position(|&x| x == c) {
                    f.remove(i);
                }
            }
            if h.occupants() == 0 {
                crate::people::status::set_icon(&mut h.status, 6, false, false);
                crate::people::status::set_icon(&mut h.status, 7, false, false);
            }
        }
    }

    /// Make `guardian` the guardian of `c`.
    ///
    /// Original: `FUN_140117470`.
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
    pub(super) fn add_dependent(&mut self, g: usize, c: usize) {
        self.states[g].dependents.push(c);
        self.citizens[c].parent = Some(g);
        let (home, slot) = (self.states[g].home, self.states[g].home_slot);
        self.set_home(c, home, slot);
    }

    /// `FUN_140117510`: leave `g`'s ring; homeless and without a guardian.
    pub(super) fn remove_dependent(&mut self, g: usize, c: usize) {
        self.states[g].dependents.retain(|&d| d != c);
        self.set_home(c, None, -1);
        self.citizens[c].parent = None;
    }

    /// `FUN_140115310`: the first registered single of the other sex old enough to marry and
    /// close enough in age.
    pub(super) fn find_partner(&self, c: usize) -> Option<usize> {
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
    pub(super) fn find_home(&mut self, c: usize, shared: bool) -> Option<(usize, i32)> {
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
                let h = self.states[o].home?;
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

    /// The birth part of `FUN_140118340`.
    pub(super) fn pregnancy(&mut self, c: usize, secs: f32) {
        let r = self.rules;
        // jb at 0x140118d5d: a NaN age is skipped, as a child's is.
        #[allow(clippy::neg_cmp_op_on_partial_ord)]
        if self.citizens[c].male
            || !(r.adult_age() <= self.citizens[c].age)
            || self.states[c].school.is_some()
        {
            return;
        }
        let s = &mut self.states[c];
        if (r.hunger_warning() < s.hunger || r.cold_warning() < s.cold) && 0.0 < s.reproduction {
            s.reproduction = 0.0;
        }
        let t = s.reproduction;
        // jbe at 0x140118dda: only a time above 0 counts down; a NaN one is left as it is.
        if 0.0 < t {
            s.reproduction = t - secs;
            if t - secs <= 0.0 {
                let health = s.health;
                let threshold = (health / r.f(0x70)) * (r.f(0x64) - r.f(0x68)) + r.f(0x68);
                // jbe at 0x140118e2a: a NaN threshold gives a birth.
                #[allow(clippy::neg_cmp_op_on_partial_ord)]
                let born = !(self.rng.rand_float() < threshold);
                if born {
                    self.give_birth(c);
                    let cooldown = self.rng.rand_float() * r.f(0x60) + r.f(0x5c);
                    self.states[c].reproduction = -cooldown;
                } else if self.die(c, reason::CHILDBIRTH, None) {
                    return;
                }
            }
        } else if t < 0.0 {
            // The time after a birth counts up to 0.
            s.reproduction = t + secs;
            if 0.0 <= t + secs {
                s.reproduction = 0.0;
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
    pub(super) fn give_birth(&mut self, mother: usize) {
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
}
