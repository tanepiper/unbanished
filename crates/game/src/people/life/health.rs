//! Health, happiness and disease: their update, an outbreak, infection, spreading and recovery.

use super::*;

impl People<'_, '_> {
    /// `FUN_140117e60`: health from the last interval's events.
    pub(super) fn health(&mut self, c: usize) {
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
    pub(super) fn happiness(&mut self, c: usize) {
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
            // Citizen `+0x84` against `_workForHappiness` (`+0xb0`, 10): nothing in 1.0.7
            // writes `+0x84` (no store in the Application but the zeroed component), so the
            // original gives the half point every time, as here.
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

    /// The citizen's disease update: the check for an outbreak when healthy
    /// ([`outbreak_check`]), else the illness running its course and spreading; false when the
    /// disease kills the citizen.
    ///
    /// Original: `FUN_140117ca0`.
    pub(super) fn disease(&mut self, c: usize, dt: i64) -> bool {
        let r = self.rules;
        match self.states[c].disease {
            None => {
                let s = &mut self.states[c];
                let health = s.health;
                let falls_ill = outbreak_check(
                    r,
                    health,
                    &mut s.immunity,
                    &mut s.disease_clock,
                    self.town_disease,
                    dt,
                    self.rng,
                );
                if falls_ill {
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
    pub(super) fn outbreak(&mut self, c: usize) {
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

    /// `FUN_14011c060`: catch disease `d` (not while immune, nor at work in a profession
    /// with `+0x4d`, the doctor). The outbreak event asks AutoSlow through the event log.
    pub(super) fn infect(&mut self, c: usize, d: usize) {
        let s = &self.states[c];
        if s.disease.is_some() || 0 < s.immunity {
            return;
        }
        let at_work = s.workplace.is_some()
            && self
                .professions
                .list
                .get(s.profession)
                .is_some_and(|p| p.immune_at_work);
        if at_work {
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

    /// `FUN_140117940`: with the disease's odds, every other citizen within its radius of the
    /// sick one (`FUN_140097e60` with the CitizenComponent type, in the characters' tree's
    /// order) who is not sick catches it (`FUN_14011c060`, which turns away the immune).
    pub(super) fn spread(&mut self, c: usize) {
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
        let near: Vec<usize> = self
            .tree
            .within(p, disease.spread_radius, |w| {
                matches!(w, crate::behaviour::character_tree::Who::Citizen(_))
            })
            .into_iter()
            .filter_map(|w| match w {
                crate::behaviour::character_tree::Who::Citizen(i) if i != c => Some(i),
                _ => None,
            })
            .collect();
        for i in near {
            if self.states.get(i).is_some_and(|s| s.disease.is_none()) {
                self.infect(i, d);
            }
        }
    }

    /// `FUN_140117880`: when the illness ends, recover (immune for `D[0x9c]` minutes) or die.
    pub(super) fn recover(&mut self, c: usize) -> bool {
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

// The weakness is 1 − health / D+0x70; the odds mix D+0x7c..+0x90 by it and by the town's disease.
/// The disease check of a citizen with no disease: the immunity runs down by `dt`, and the odds
/// come from the citizen's weakness and the town's disease. With odds of 0 or less the clock is
/// cleared and nothing is drawn. Otherwise one `RandFloat` against the time on the clock over the
/// odds' span, and the clock is cleared. True when the citizen falls ill (the caller starts the
/// outbreak).
///
/// Measured: `tools/emu/disease_check.py` (2,000 cases; the original draws exactly when the
/// odds are above 0).
///
/// Original: `FUN_140117ca0` (its first branch).
pub fn outbreak_check(
    r: &LifeRules,
    health: f32,
    immunity: &mut i64,
    clock: &mut i64,
    town_disease: f32,
    dt: i64,
    rng: &mut Random,
) -> bool {
    // CitizenComponent +0x70.
    *immunity = (*immunity - dt).max(0);
    let weak = 1.0 - health / r.f(0x70);
    let a = (r.f(0x84) - r.f(0x7c)) * weak + r.f(0x7c);
    let b = (r.f(0x88) - r.f(0x80)) * weak + r.f(0x80);
    let a = (r.f(0x8c) - a) * town_disease + a;
    let b = (r.f(0x90) - b) * town_disease + b;
    // CitizenComponent +0x60: odds of 0 or less clear the clock and return before the draw.
    if b <= 0.0 {
        *clock = 0;
        return false;
    }
    let elapsed = *clock;
    *clock = 0;
    let span = a * 12.0 * MINUTE * b;
    rng.rand_float() < seconds(elapsed) / span
}
