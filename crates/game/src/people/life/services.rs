//! Workplaces and services: looking for a workplace, choosing and leaving a service (school, chapel, healer, tavern), lessons and treatment.

use super::*;

impl People<'_, '_> {
    /// `FUN_140118340`'s workplace search: without a workplace, or when `FUN_140115eb0` says
    /// to look again, `FUN_140115450` chooses one (or none) and `FUN_1401173b0` moves there.
    pub(super) fn search_workplace(&mut self, c: usize) {
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
        let mut search = crate::behaviour::activities::TownSearch {
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

    /// The path-rank distance (`FUN_140008830`).
    pub(super) fn rank(&mut self, a: i32, b: i32) -> i32 {
        let (size, terrain, floors) = (self.town.size, self.terrain, self.town.floors);
        let ground = move |cell: i32| crate::town::floor_ground(floors, size, terrain, cell);
        let mut g = crate::behaviour::activities::StorageGeometry {
            town: self.town,
            sectors: self.sectors,
            ground: &ground,
        };
        crate::work::stock::StorageSearchMap::rank(&mut g, a, b)
    }

    /// `FUN_140044a50`: room, a worker, and the building reachable from the citizen.
    pub(super) fn accepts(&self, e: usize, c: usize) -> bool {
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
    pub(super) fn find_service(
        &mut self,
        c: usize,
        service: usize,
        exclude: Option<usize>,
    ) -> Option<usize> {
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

    // +0x220
    /// Join the nearest school, chapel or doctor; the school also sets.
    ///
    /// Original: `FUN_140116d10`, `FUN_140116bc0`, `FUN_140116a60`.
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

    /// Leave; the school can be swapped for another at once (`again`).
    ///
    /// Original: `FUN_14011af20`, `FUN_140116b20`, `FUN_14011ae80`.
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

    /// Citizen message 4: a lesson; at `D[0x118]` lessons, or after `D[0x11c]` hours at school,
    /// educated (flag `0x400`), out of school and an adult (9).
    ///
    /// Original: `FUN_140116c70`.
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

    /// Citizen message 5: the doctor shortens the illness by `D[0x12c]` minutes.
    ///
    /// Original: `FUN_140116a10`.
    pub fn treated(&mut self, c: usize) {
        if self.states[c].disease.is_some() {
            self.states[c].disease_left -= ticks(self.rules.f(0x12c) * MINUTE);
        }
    }
}
