//! A citizen's update: ageing, needs, health and the events they raise.

use super::*;

impl People<'_, '_> {
    /// One scheduled update of citizen `c` over `dt`. Not ported: the debug flags that turn
    /// hunger and cold off and make every adult educated.
    ///
    /// Original: `FUN_140118340`.
    // Game +0xd0 bits 1, 2, 9
    pub fn update(&mut self, c: usize, dt: i64) {
        let r = self.rules;
        let secs = seconds(dt);
        let adult_age = r.adult_age();
        // jb at 0x14011839e: a NaN age counts as a child.
        #[allow(clippy::neg_cmp_op_on_partial_ord)]
        let was_child = !(adult_age <= self.citizens[c].age) || self.states[c].school.is_some();
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
        // jbe at 0x14011852b: warm enough, or a NaN temperature, clears the cold.
        #[allow(clippy::neg_cmp_op_on_partial_ord)]
        if !(self.temperature < r.f(0xf4)) {
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
            crate::people::status::set_icon(&mut s.status, 0, s.home.is_none(), false);
            crate::people::status::set_icon(&mut s.status, 1, warn_hunger < s.hunger, false);
            crate::people::status::set_icon(&mut s.status, 2, warn_cold < s.cold, false);
            // Icon 3: no workplace, for a profession that does not work anywhere
            // (profession `+0x4c` clear, citizen `+0x248` unset).
            let unplaced = !self.professions.works_anywhere(s.profession) && s.workplace.is_none();
            crate::people::status::set_icon(&mut s.status, 3, unplaced, false);
            crate::people::status::set_icon(&mut s.status, 5, s.disease.is_some(), false);
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
        // jb at 0x1401189a8 and 0x140118a71: a NaN age counts as a child.
        #[allow(clippy::neg_cmp_op_on_partial_ord)]
        let child = !(adult_age <= self.citizens[c].age) || self.states[c].school.is_some();
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
}
