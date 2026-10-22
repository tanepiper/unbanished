//! Offering work to a citizen: the job lists in order, emergencies and sorting by distance.

use super::*;

impl Jobs {
    /// Slot `+0x20` of a work taken out of the table, answering [`NESTED`] by offering the
    /// queued works (`FUN_1400fdaf0` without a second try) until one is taken: then 2, else the
    /// rest of the try (which may offer again). The work is back in the table afterwards.
    pub(super) fn try_one(
        &mut self,
        id: i32,
        mut work: Work,
        citizen: usize,
        host: &mut impl WorkHost,
    ) -> u8 {
        let mut r = host.try_assign(&mut work, citizen, self.time);
        self.works.insert(id, work);
        while r == NESTED {
            let again = host.offers_again();
            for other in host.take_offers() {
                if self.offer_one(other, citizen, again, host) {
                    return 2;
                }
            }
            let Some(mut work) = self.works.remove(&id) else {
                return 1;
            };
            r = host.resume(&mut work, citizen);
            self.works.insert(id, work);
        }
        r
    }

    /// Offer one work by id, with a second try when the first answers "not now" and `again` is set.
    ///
    /// Original: `FUN_1400fdaf0`.
    pub fn offer_one(
        &mut self,
        id: i32,
        citizen: usize,
        again: bool,
        host: &mut impl WorkHost,
    ) -> bool {
        let Some(mut work) = self.works.remove(&id) else {
            return false;
        };
        let mut taken = false;
        if work.failed_pass != self.pass {
            host.refresh(&mut work);
            match self.try_one(id, work, citizen, host) {
                2 => taken = true,
                1 => {
                    if again {
                        if let Some(mut work) = self.works.remove(&id) {
                            host.refresh(&mut work);
                            taken = host.try_assign_again(&mut work, citizen, self.time);
                            self.works.insert(id, work);
                        }
                    }
                    if !taken {
                        if let Some(w) = self.works.get_mut(&id) {
                            w.failed_pass = self.pass;
                        }
                    }
                }
                _ => {}
            }
        } else {
            self.works.insert(id, work);
        }
        self.drain(host);
        taken
    }

    /// Give a citizen work: activity 9.
    ///
    /// Original: `FUN_1400fcd00`.
    pub fn assign_work(&mut self, citizen: usize, host: &mut impl WorkHost) {
        if self.works.is_empty() {
            return;
        }
        let laborer = host.laborer();
        let profession = host.profession(citizen);
        if profession != laborer {
            if let Some(set) = self.professions.get(&profession).cloned() {
                if self.try_list(citizen, &set, profession, false, false, host) {
                    return;
                }
            }
        }
        if !self.assign_emergency(citizen, host) {
            if let Some(set) = self.professions.get(&laborer).cloned() {
                self.try_list(citizen, &set, laborer, false, false, host);
            }
        }
    }

    /// Give a citizen emergency work (activity 2), for example fighting a fire. Returns whether
    /// there was some.
    ///
    /// Original: `FUN_1400fca20`.
    pub fn assign_emergency(&mut self, citizen: usize, host: &mut impl WorkHost) -> bool {
        let mut lists: Vec<&[u32]> = Vec::new();
        let emergency = host.has_emergency(citizen);
        if emergency {
            lists.push(&[EMERGENCY]);
        } else {
            if host.profession(citizen) == host.builder() {
                lists.push(&[CONSTRUCTION]);
            }
            for (slot, categories) in EMERGENCY_LISTS {
                let (count, limit) = host.stock(slot);
                if count <= limit / 10 {
                    lists.push(categories);
                }
            }
        }
        let k = host.rng().rand_below(lists.len() as u32) as usize;
        let Some(list) = lists.get(k).copied() else {
            return false;
        };
        let profession = host.profession(citizen);
        for &c in list {
            let set = self.categories[c as usize - 1].clone();
            if self.try_list(citizen, &set, profession, true, emergency, host) {
                return true;
            }
        }
        false
    }

    /// `FUN_1400fce80`: the citizen's workplace's works first (when it has the list's
    /// profession), then, for professions that work anywhere (or in an emergency), all.
    pub(super) fn try_list(
        &mut self,
        citizen: usize,
        set: &BTreeSet<i32>,
        profession: usize,
        by_distance: bool,
        emergency: bool,
        host: &mut impl WorkHost,
    ) -> bool {
        let workplace = if emergency {
            None
        } else {
            host.workplace(citizen)
        };
        if let Some(wp) = workplace {
            if profession == host.profession(citizen) {
                let order = self.sort(citizen, set, Some(wp), by_distance, host);
                if self.offer(citizen, &order, host) {
                    return true;
                }
            }
        }
        if host.works_anywhere(profession) || emergency {
            let order = self.sort(citizen, set, None, by_distance, host);
            if self.offer(citizen, &order, host) {
                return true;
            }
        }
        false
    }

    /// `FUN_1400fd160` / `FUN_1400fd4d0`: the works of `set` (of entity `owner` when given),
    /// ordered by path rank from the citizen's cell or by priority, then by id.
    pub(super) fn sort(
        &self,
        citizen: usize,
        set: &BTreeSet<i32>,
        owner: Option<usize>,
        by_distance: bool,
        host: &mut impl WorkHost,
    ) -> Vec<i32> {
        let from = host.cell(citizen);
        let mut keyed: Vec<(i32, i32)> = set
            .iter()
            .filter_map(|id| self.works.get(id))
            .filter(|w| owner.is_none_or(|o| w.entity == o))
            .map(|w| {
                let key = if by_distance {
                    host.rank(from, w.cell)
                } else {
                    w.priority
                };
                (key, w.id)
            })
            .collect();
        keyed.sort();
        keyed.dedup();
        keyed.into_iter().map(|(_, id)| id).collect()
    }

    /// `FUN_1400fd030`.
    pub(super) fn offer(
        &mut self,
        citizen: usize,
        order: &[i32],
        host: &mut impl WorkHost,
    ) -> bool {
        for &id in order {
            let Some(mut work) = self.works.remove(&id) else {
                continue;
            };
            let mut taken = false;
            if work.failed_pass != self.pass {
                host.refresh(&mut work);
                match self.try_one(id, work, citizen, host) {
                    2 => taken = true,
                    1 => {
                        if let Some(w) = self.works.get_mut(&id) {
                            w.failed_pass = self.pass;
                        }
                    }
                    _ => {}
                }
            } else {
                self.works.insert(id, work);
            }
            self.drain(host);
            if taken {
                return true;
            }
        }
        false
    }
}
