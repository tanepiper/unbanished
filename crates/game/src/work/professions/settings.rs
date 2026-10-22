//! Counts, capacity and the player's settings per profession and workplace, and choosing a workplace.

use super::*;

impl Workforce {
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
}
