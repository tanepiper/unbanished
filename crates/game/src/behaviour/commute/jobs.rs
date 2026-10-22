//! Moving citizens to jobs nearer their homes: scoring a job and reassigning workers.

use super::*;

impl Sim<'_, '_> {
    /// `FUN_1400db530`: the commute from `home` to a workplace entrance: far without a home or
    /// a way; else the region pair's rank. A pair not yet ranked is ranked now (`FUN_140008830`)
    /// but counts 0 this time.
    pub(super) fn job_score(
        &mut self,
        ranks: &mut BTreeMap<(i32, i32), i32>,
        home: i32,
        to: i32,
    ) -> i32 {
        if home < 0 || !self.reachable(home, to) {
            return FAR;
        }
        if self.region(home) < 0 || self.region(to) < 0 {
            let d = self.straight(home, to);
            return if d < 0 { FAR } else { d };
        }
        let key = self.region_key(home, to);
        let score = match ranks.get(&key) {
            Some(&r) => r,
            None => {
                let mut r = self.rank_or_distance(home, to);
                if (r as u32) < 2 {
                    r = self.straight(home, to);
                }
                ranks.insert(key, r);
                0
            }
        };
        if score < 0 {
            FAR
        } else {
            score
        }
    }

    /// A citizen's home use point, −1 without a home.
    pub(super) fn home_cell(&self, c: usize) -> i32 {
        self.states[c].home.map_or(-1, |h| self.point_cell(h))
    }

    /// WorkPlaceComponent `+0x48`.
    pub(super) fn place_profession(&self, wp: usize) -> Option<usize> {
        self.workforce
            .workplaces
            .get(&wp)
            .and_then(|w| w.profession)
    }

    /// `FUN_1401173b0` and the profession, when either differs.
    pub(super) fn assign(
        &mut self,
        workers: &mut [crate::work::professions::Worker],
        c: usize,
        profession: usize,
        wp: Option<usize>,
    ) {
        if workers[c].profession != profession || workers[c].workplace != wp {
            self.workforce.set_workplace(workers, c, wp);
            workers[c].profession = profession;
        }
    }

    /// `FUN_1400d9880`.
    pub(super) fn commute_jobs(&mut self, job: &mut Commute) {
        let laborer = self.professions.laborer;
        let mut workers = self.workers();
        let n = workers.len();
        // A worker whose workplace takes another profession becomes a laborer.
        for c in 0..n {
            let Some(wp) = workers[c].workplace.filter(|_| workers[c].adult) else {
                continue;
            };
            if self.place_profession(wp) != Some(workers[c].profession) {
                self.assign(&mut workers, c, laborer, None);
            }
        }
        // The workplaces with workers, by profession then registry order.
        let mut places = Vec::new();
        for p in 0..self.professions.list.len() {
            for w in self.workforce.registry.get(&p).cloned().unwrap_or_default() {
                let Some(place) = self.workforce.workplaces.get(&w) else {
                    continue;
                };
                let count = place.workers.len() as i32;
                let profession = place.profession;
                if count == 0 {
                    continue;
                }
                let entrance = self.entrance_cell(w);
                places.push(Place {
                    entity: w,
                    profession,
                    entrance,
                    left: count,
                    total: count,
                    candidates: BTreeSet::new(),
                });
            }
        }
        // Every working adult is let go and scored against each workplace; the professions
        // are counted.
        let mut free = BTreeSet::new();
        let mut quota: BTreeMap<usize, i32> = BTreeMap::new();
        for c in 0..n {
            if !workers[c].adult {
                continue;
            }
            free.insert(c);
            if let Some(wp) = workers[c].workplace {
                if self.place_profession(wp) != Some(workers[c].profession) {
                    self.assign(&mut workers, c, laborer, None);
                }
            }
            *quota.entry(workers[c].profession).or_insert(0) += 1;
            self.assign(&mut workers, c, laborer, None);
            let home = self.home_cell(c);
            for place in places.iter_mut() {
                let score = self.job_score(&mut job.ranks, home, place.entrance);
                place.candidates.insert((score, c));
            }
        }
        // The queue: the highest priority first, then the first record.
        let mut queue = BTreeSet::new();
        for (i, place) in places.iter().enumerate() {
            if let Some(&(score, _)) = place.candidates.first() {
                queue.insert((Reverse(place.priority(score)), i));
            }
        }
        while let Some(&(key, i)) = queue.first() {
            queue.remove(&(key, i));
            let Some(&(_, c)) = places[i].candidates.first() else {
                continue;
            };
            let (entity, profession) = (places[i].entity, places[i].profession);
            if let Some(p) = profession {
                self.assign(&mut workers, c, p, Some(entity));
            }
            let key = profession.unwrap_or(usize::MAX);
            if let Some(q) = quota.get_mut(&key) {
                *q -= 1;
                if *q == 0 {
                    quota.remove(&key);
                }
            }
            free.remove(&c);
            let home = self.home_cell(c);
            places[i].left -= 1;
            for j in 0..places.len() {
                if places[j].left == 0 {
                    continue;
                }
                let score = self.job_score(&mut job.ranks, home, places[j].entrance);
                let entry = if places[j].candidates.contains(&(score, c)) {
                    Some((score, c))
                } else {
                    places[j].candidates.iter().copied().find(|e| e.1 == c)
                };
                let Some(entry) = entry else {
                    continue;
                };
                if places[j].candidates.first() == Some(&entry) {
                    let old = places[j].priority(entry.0);
                    queue.remove(&(Reverse(old), j));
                    places[j].candidates.remove(&entry);
                    if let Some(&(best, _)) = places[j].candidates.first() {
                        queue.insert((Reverse(places[j].priority(best)), j));
                    }
                } else {
                    places[j].candidates.remove(&entry);
                }
            }
        }
        // The professions left over, the first citizens to the first professions.
        while let Some((&p, _)) = quota.first_key_value() {
            let Some(c) = free.pop_first() else {
                break;
            };
            self.assign(&mut workers, c, p, None);
            let q = quota.get_mut(&p).unwrap();
            *q -= 1;
            if *q == 0 {
                quota.remove(&p);
            }
        }
        self.set_workers(&workers);
    }
}
