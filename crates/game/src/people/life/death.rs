//! Death and burial: dying with a reason, finding a cemetery and a free grave, and burying.

use super::*;

impl People<'_, '_> {
    // Game +0xd0
    /// Queue the citizen's death (`reason`) and tell the family. When the citizen is buried in a
    /// cemetery (found from the home's entrance, else from where the citizen stands), the
    /// family's bit is `0x40`; otherwise it is `2` for old age, else `0x80`. Not ported: the debug
    /// flag that stops deaths (`Game` bit 8).
    ///
    /// Original: `FUN_14011c2a0`; the cemetery search is `FUN_14008ec10`.
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
                    // FUN_1400473f0 makes the tombstone here; its setup (FUN_1400581c0) draws the
                    // lifetime right after the grave's draw.
                    let life = self.site_templates[self.sites[&cemetery].template]
                        .cemetery
                        .map_or(0, |t| crate::people::cemetery::tombstone_life(&t, self.rng));
                    self.burials.push((cemetery, cell, life));
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
    pub(super) fn find_cemetery(&mut self, cell: i32) -> Option<usize> {
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
    pub(super) fn free_grave(&mut self, cemetery: usize) -> Option<usize> {
        let graves = &self.sites.get(&cemetery)?.graves;
        let count = graves.len();
        let start = self.rng.rand_below(count as u32) as usize;
        (0..count).map(|i| (start + i) % count).find(|&i| {
            let cell = graves[i];
            let taken = self.town.objects[cell as usize]
                .is_some_and(|o| self.tombstones.contains_key(&o))
                || self.burials.iter().any(|&(_, b, _)| b == cell);
            !taken
        })
    }

    /// The family part of a citizen's removal: the citizen leaves school, the doctor, the chapel,
    /// the home and the guardian. A widow or widower takes the dependents; otherwise children die
    /// (orphaned) and older dependents leave. The partner is single again.
    ///
    /// Original: `FUN_140119170` (the removal of a `CitizenComponent`).
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
}
