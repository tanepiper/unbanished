//! The town's commute optimisation, run from StockManager::Update every 300 s of game time. See
//! docs/COMMUTE.md.
//!
//! - Start: one cell pair (residence entrance, workplace entrance) per distinct pair of map
//!   regions, each region pair cached with rank 0.
//! - Step (every update): up to seven pairs, last first, get their path rank (`Map::PathRank` mode
//!   0; 0 or 1 become the straight distance). With none left:
//! - Homes: families in single-family residences, the longest commute first, move to the empty
//!   residence that shortens theirs most, goods and all.
//! - Jobs: every working adult is let go, then each workplace takes back as many as it had, the
//!   workplace whose nearest candidate is furthest (weighted by how much of it is still to fill)
//!   choosing first; the rest get the professions left over.
//!
//! Pointer orders in the original's trees (citizens, residences, professions) are taken as index
//! order here.
//!
//! Original: `FUN_140091780`, `FUN_1400dc590`, `FUN_1400dc3d0`, `FUN_1400db770`, `FUN_1400d9880`.

mod jobs;

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};

use crate::people::citizens::Citizen;
use crate::scheduler::ticks;
use crate::town::Sim;

/// `DAT_140178934`: seconds between runs.
const PERIOD_SECONDS: f32 = 300.0;
/// The cost of an unreachable or homeless commute (`0x1000000`).
const FAR: i32 = 0x100_0000;
/// The pairs ranked per update (`min(index, 6) + 1`).
const STEP: i32 = 7;

// +0x190; +0x1b8
/// The optimisation's state: StockManager (the time since the last run) and the job object (its
/// pairs, the next pair's index and the region-pair ranks).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commute {
    /// The time since the last run.
    pub timer: i64,
    /// The pairs of cells to rank: a workplace entrance and a home.
    pub pairs: Vec<(i32, i32)>,
    /// The next pair to rank (−1 when done).
    pub index: i32,
    /// The rank of each pair of regions.
    pub ranks: BTreeMap<(i32, i32), i32>,
}

impl Default for Commute {
    fn default() -> Self {
        Commute {
            timer: 0,
            pairs: Vec::new(),
            index: -1,
            ranks: BTreeMap::new(),
        }
    }
}

/// A workplace while the jobs are dealt (`0x28`-byte records): its entity, profession,
/// entrance, places left and total (its workers at the start), and the candidates by score.
struct Place {
    entity: usize,
    profession: Option<usize>,
    entrance: i32,
    left: i32,
    total: i32,
    candidates: BTreeSet<(i32, usize)>,
}

impl Place {
    /// The queue priority: the best candidate's score times the share still to fill.
    fn priority(&self, score: i32) -> i32 {
        (score as f32 * (1.0 - (self.total - self.left) as f32 / self.total as f32)) as i32
    }
}

impl Sim<'_, '_> {
    /// Map cell `+0x18`: the region (port 0 is none, the original's −1).
    fn region(&self, cell: i32) -> i32 {
        match self.regions.get(cell as usize) {
            Some(&r) if r != 0 => r as i32,
            _ => -1,
        }
    }

    fn region_key(&self, a: i32, b: i32) -> (i32, i32) {
        let (ra, rb) = (self.region(a), self.region(b));
        (ra.max(rb), ra.min(rb))
    }

    /// `(int)sqrtf(dy² + dx²)` in cells.
    fn straight(&self, a: i32, b: i32) -> i32 {
        let n = self.size as i32;
        let (dy, dx) = (a / n - b / n, a % n - b % n);
        ((dy * dy + dx * dx) as f32).sqrt() as i32
    }

    /// StockManager::Update's part: the clock, then a step, or a new start when idle.
    pub fn update_commute(&mut self, job: &mut Commute, citizens: &[Citizen], dt: i64) {
        job.timer += dt;
        let period = ticks(PERIOD_SECONDS);
        if job.timer < period {
            return;
        }
        if !self.commute_step(job, citizens) {
            self.commute_start(job);
        } else if job.index < 0 && period < job.timer {
            loop {
                job.timer -= period;
                if job.timer <= period {
                    break;
                }
            }
        }
    }

    /// `FUN_1400dc590` (only while idle).
    fn commute_start(&mut self, job: &mut Commute) {
        if 0 <= job.index {
            return;
        }
        let residences = self.workforce.residences.clone();
        let homes: Vec<i32> = residences.iter().map(|&r| self.entrance_cell(r)).collect();
        job.ranks.clear();
        job.pairs.clear();
        for p in 0..self.professions.list.len() {
            let places = self.workforce.registry.get(&p).cloned().unwrap_or_default();
            for w in places {
                let entrance = self.entrance_cell(w);
                if self.region(entrance) < 0 {
                    continue;
                }
                for &home in &homes {
                    if self.region(home) < 0 {
                        continue;
                    }
                    let key = self.region_key(entrance, home);
                    if job.ranks.contains_key(&key) {
                        continue;
                    }
                    job.pairs.push((entrance.max(home), entrance.min(home)));
                    job.ranks.insert(key, 0);
                }
            }
        }
        job.index = job.pairs.len() as i32 - 1;
    }

    /// `FUN_1400dc3d0`: false while idle; with the last pair ranked, the homes and the jobs.
    fn commute_step(&mut self, job: &mut Commute, citizens: &[Citizen]) -> bool {
        if job.index < 0 {
            return false;
        }
        for _ in 0..job.index.min(STEP - 1) + 1 {
            let (a, b) = job.pairs[job.index as usize];
            let mut rank = self.path_rank(a, b, false);
            if (rank as u32) < 2 {
                rank = self.straight(a, b);
            }
            let key = self.region_key(a, b);
            job.ranks.insert(key, rank);
            job.index -= 1;
        }
        if job.index < 0 {
            self.commute_homes(citizens);
            self.commute_jobs(job);
            job.pairs.clear();
        }
        true
    }

    /// `FUN_1400dc130`: the commute of `family`'s working adults (no laborers, students or
    /// unreachable workplaces) from `home`'s use point: the sum of the path ranks
    /// (`FUN_140008830`).
    fn family_cost(&mut self, family: usize, home: usize) -> i32 {
        let from = self.point_cell(home);
        let laborer = self.professions.laborer;
        let members = self
            .homes
            .get(&family)
            .map(|h| h.families[0].clone())
            .unwrap_or_default();
        let mut cost = 0i32;
        for c in members {
            let s = &self.states[c];
            if !self.adult[c] || s.profession == laborer {
                continue;
            }
            let Some(wp) = s.workplace else {
                continue;
            };
            let to = self.entrance_cell(wp);
            if self.reachable(from, to) {
                cost = cost.wrapping_add(self.rank_or_distance(from, to));
            }
        }
        cost
    }

    /// `FUN_1400db770`.
    fn commute_homes(&mut self, citizens: &[Citizen]) {
        let mut empty = BTreeSet::new();
        // Longest commute first; equal ones by residence.
        let mut occupied = BTreeSet::new();
        for r in self.workforce.residences.clone() {
            let Some(h) = self.homes.get(&r) else {
                continue;
            };
            if 2 <= h.families.len() {
                continue;
            }
            if h.families[0].is_empty() {
                empty.insert(r);
            } else {
                let cost = self.family_cost(r, r);
                occupied.insert((Reverse(cost), r));
            }
        }
        if empty.is_empty() {
            return;
        }
        for (Reverse(cost), from) in occupied {
            let mut best = (cost, None);
            for &to in &empty {
                let c = self.family_cost(from, to);
                if c < best.0 {
                    best = (c, Some(to));
                }
            }
            let Some(to) = best.1 else {
                continue;
            };
            // `FUN_14011b8a0` for each member, over the live list.
            let mut i = 0;
            while let Some(&c) = self.homes.get(&from).and_then(|h| h.families[0].get(i)) {
                self.move_home(citizens, c, to, 0);
                i += 1;
            }
            self.move_goods(from, to);
            empty.remove(&to);
            if empty.is_empty() {
                break;
            }
        }
    }

    /// The family's goods follow it, each item whole where it fits (`FUN_140132250`,
    /// `FUN_1401324d0`), then leave the old home (`FUN_140132620`).
    fn move_goods(&mut self, from: usize, to: usize) {
        let (Some(a), Some(b)) = (
            self.homes.get(&from).map(|h| h.storage),
            self.homes.get(&to).map(|h| h.storage),
        ) else {
            return;
        };
        let items: Vec<(usize, i32)> = self.storages[a]
            .items
            .iter()
            .map(|i| (i.material, i.count - i.reserved))
            .collect();
        let mut moved = Vec::new();
        for (m, n) in items {
            if self.storages[b].fits(self.materials, m, n) {
                self.storages[b].add(m, n);
                moved.push((m, n));
            }
        }
        for (m, n) in moved {
            self.storages[a].remove(m, n);
        }
    }

    /// `FUN_14011b8a0`: citizen `c` (and its dependents) to `home`'s family `slot`.
    fn move_home(&mut self, citizens: &[Citizen], c: usize, home: usize, slot: i32) {
        let s = &self.states[c];
        if s.home == Some(home) && s.home_slot == slot {
            return;
        }
        if let Some(old) = s.home {
            let old_slot = s.home_slot.max(0) as usize;
            if let Some(h) = self.homes.get_mut(&old) {
                if let Some(f) = h.families.get_mut(old_slot) {
                    f.retain(|&x| x != c);
                }
                if h.occupants() == 0 {
                    crate::people::status::set_icon(&mut h.status, 6, false, false);
                    crate::people::status::set_icon(&mut h.status, 7, false, false);
                }
            }
        }
        self.states[c].home = Some(home);
        self.states[c].home_slot = slot;
        if let Some(f) = self
            .homes
            .get_mut(&home)
            .and_then(|h| h.families.get_mut(slot.max(0) as usize))
        {
            f.push(c);
            f.sort_by(|&a, &b| crate::people::life::family_order(citizens, a, b));
        }
        for d in self.states[c].dependents.clone() {
            self.move_home(citizens, d, home, slot);
        }
    }
}

#[cfg(test)]
mod tests;
