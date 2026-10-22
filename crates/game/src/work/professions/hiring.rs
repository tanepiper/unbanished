//! Hiring and firing: a free workplace, who takes a job and who leaves one.

use super::*;

impl Workforce {
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
    pub(super) fn family_member(
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
    pub(super) fn nearest_laborer(
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
    pub(super) fn largest_family(
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
}
