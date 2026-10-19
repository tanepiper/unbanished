//! CitizenAIComponent (vtable `0x140168890`, constructor `FUN_140115040`): choosing what a
//! citizen does next. See docs/CITIZEN-AI.md.

use crate::random::Random;
use crate::scheduler::{seconds, ticks};

/// Activities (`FUN_1401146c0`).
pub mod activity {
    pub const EAT: u32 = 0;
    pub const WARM: u32 = 1;
    pub const EMERGENCY: u32 = 2;
    pub const CLOTHING: u32 = 3;
    pub const HERBS: u32 = 4;
    pub const DOCTOR: u32 = 5;
    pub const HOME_FOOD: u32 = 6;
    pub const HOME_FUEL: u32 = 7;
    pub const TOOL: u32 = 8;
    pub const WORK: u32 = 9;
    pub const SCHOOL: u32 = 10;
    pub const PLAY: u32 = 11;
    pub const IDLE: u32 = 12;
    pub const DROP_OFF: u32 = 13;
    pub const EAT_URGENT: u32 = 14;
    pub const WARM_URGENT: u32 = 15;
}
use activity::*;

/// `DAT_1401788fc`, `DAT_140178934`, `DAT_140178954`: loiter timers (seconds).
const LOITER_UNHAPPY: f32 = 75.0;
const LOITER_SAD: f32 = 300.0;
const LOITER_ANYWAY: f32 = 1800.0;
/// `DAT_140178934`: carrying inventory this long raises drop-off priority.
const CARRY_LONG: f32 = 300.0;

/// The citizen values candidate building reads (`D` = CitizenDescription).
#[derive(Debug, Clone, Copy, Default)]
pub struct CitizenView {
    /// `+0x68` disease, `+0x228` doctor assigned.
    pub sick: bool,
    pub doctor: bool,
    /// `+0x44` hunger, `+0x48` cold, `+0x50` health, `+0x54` bit 9, `+0x80` happiness.
    pub hunger: f32,
    pub cold: f32,
    pub health: f32,
    pub health_flag: bool,
    pub happiness: f32,
    /// `+0x3c` age; `+0x208` school; `+0x238` home; `+0x1e4` clothing; `+0xac` tool uses.
    pub age: f32,
    pub school: bool,
    pub home: bool,
    pub clothing: i32,
    pub tool: i32,
    /// `FUN_14011b0b0`: map byte `+0x22` set at home, work or here.
    pub emergency: bool,
    /// `FUN_14011abb0` / `FUN_14011acc0`: home has food / firewood.
    pub home_food: bool,
    pub home_fuel: bool,
    /// Spouse's (`+0x250`) supply errand, citizen `+0xb0`: 1 food, 2 firewood.
    pub spouse_activity: Option<u32>,
    /// `FUN_14011b2b0`: carried inventory count.
    pub carrying: i32,
}

/// The CitizenDescription thresholds candidate building reads.
#[derive(Debug, Clone, Copy, Default)]
pub struct Thresholds {
    pub adult_age: f32,     // +0x44
    pub health_low: f32,    // +0x74
    pub happy_low: f32,     // +0xb4
    pub happy_unhappy: f32, // +0xb8
    pub hungry: f32,        // +0xc8
    pub hungry_urgent: f32, // +0xcc
    pub cold: f32,          // +0xe8
    pub cold_urgent: f32,   // +0xec
}

/// One candidate (12-byte entries: activity, priority, unused).
pub type Candidate = (u32, i32);

#[derive(Debug, Clone)]
pub struct CitizenAi {
    /// `+0x28` current, `+0x2c` queued (-1).
    pub current: u32,
    pub queued: Option<u32>,
    /// `+0x30` / `+0x31`: urgently eating / warming.
    pub eating: bool,
    pub warming: bool,
    /// `+0x40..+0xb8`: per-activity cooldowns (ticks).
    pub cooldowns: [i64; 16],
    /// `+0xc0`: time since the last loiter; `+0xc8`: time carrying inventory.
    pub since_loiter: i64,
    pub carrying_time: i64,
}

impl Default for CitizenAi {
    fn default() -> Self {
        CitizenAi {
            current: IDLE,
            queued: None,
            eating: false,
            warming: false,
            cooldowns: [0; 16],
            since_loiter: 0,
            carrying_time: 0,
        }
    }
}

/// `FUN_140113170`: candidate activities in push order, and whether the list is shuffled and
/// sorted (the doctor-only and urgent-only lists return before that).
pub fn candidates(
    ai: &CitizenAi,
    c: &CitizenView,
    d: &Thresholds,
    urgent_only: bool,
) -> (Vec<Candidate>, bool) {
    let (list, early) = candidate_list(ai, c, d, urgent_only);
    (list, !early)
}

fn candidate_list(
    ai: &CitizenAi,
    c: &CitizenView,
    d: &Thresholds,
    urgent_only: bool,
) -> (Vec<Candidate>, bool) {
    let mut out = Vec::new();
    if c.sick && c.doctor {
        out.push((DOCTOR, 6));
        return (out, true);
    }
    if c.hunger > d.hungry_urgent {
        out.push((EAT_URGENT, 7));
    }
    if c.cold > d.cold_urgent {
        out.push((WARM_URGENT, 7));
    }
    if c.hunger > d.hungry {
        out.push((EAT, 6));
    }
    if c.cold > d.cold {
        out.push((WARM, 6));
    }
    if !c.sick {
        if c.emergency {
            out.push((EMERGENCY, 7));
        }
        if c.clothing == 0 {
            out.push((CLOTHING, 3));
        }
        if c.health < d.health_low && !c.health_flag {
            out.push((HERBS, 3));
        }
    } else {
        out.push((DOCTOR, 6));
    }
    if urgent_only {
        return (out, true);
    }
    let loiter = seconds(ai.since_loiter);
    let restless = (c.happiness < d.happy_unhappy && loiter > LOITER_UNHAPPY)
        || (c.happiness < d.happy_low && loiter > LOITER_SAD)
        || loiter > LOITER_ANYWAY;
    out.push((IDLE, if restless { 2 } else { 0 }));
    if c.age < d.adult_age || c.school {
        if !c.school {
            if !c.sick {
                out.push((PLAY, 0));
            }
            if !c.home_food {
                out.push((HOME_FOOD, 4));
            }
            if c.home_fuel {
                return (out, false);
            }
            out.push((HOME_FUEL, 4));
        } else if !c.sick {
            out.push((SCHOOL, 2));
        }
        return (out, false);
    }
    if c.home {
        if !c.home_food && c.spouse_activity != Some(1) {
            out.push((HOME_FOOD, if c.hunger > d.hungry_urgent { 6 } else { 4 }));
        }
        if !c.home_fuel && c.spouse_activity != Some(2) {
            out.push((HOME_FUEL, if c.cold > d.cold_urgent { 6 } else { 4 }));
        }
    }
    if c.sick {
        return (out, false);
    }
    if c.tool < 1 {
        out.push((TOOL, 5));
    }
    out.push((WORK, 2));
    if c.carrying >= 1 {
        let long = ticks(CARRY_LONG) < ai.carrying_time;
        out.push((DROP_OFF, if long { 3 } else { 1 }));
    }
    (out, false)
}

/// `FUN_140113170`'s tail: swap each position with `Rand(count)`, then sort.
pub fn shuffle_and_sort(list: &mut [Candidate], rng: &mut Random) {
    let n = list.len() as u32;
    for i in 0..list.len() {
        let j = rng.rand_below(n) as usize;
        if j != i {
            list.swap(i, j);
        }
    }
    if list.len() > 1 {
        crt_qsort(list);
    }
}

/// `FUN_140030770`: the MSVC CRT `qsort` inlined for candidates, ordering by priority, highest
/// first (comparator `b.priority - a.priority`). Median of three, short sort (selection of the
/// maximum) for 8 or fewer elements, smaller partition first.
pub fn crt_qsort(a: &mut [Candidate]) {
    crt_qsort_by(a, |a: &Candidate, b: &Candidate| b.1 - a.1);
}

/// The MSVC CRT `qsort` (as `FUN_140030770` inlines it) with any comparator; the other inlined
/// copies (`FUN_14002c9d0`, the library `qsort`) are the same algorithm.
/// The templated copies with a `less` comparator (`FUN_140030c00`, `FUN_1400220f0`,
/// `FUN_14001f120`, `FUN_140015c10`, `FUN_140015f10`, `FUN_140016ce0`, `FUN_140016fe0`): the
/// same algorithm, `cmp(a, b) > 0` being `less(b, a)` and equality neither way.
pub fn crt_qsort_less<T>(a: &mut [T], less: impl Fn(&T, &T) -> bool) {
    crt_qsort_by(a, |x, y| {
        if less(y, x) {
            1
        } else if less(x, y) {
            -1
        } else {
            0
        }
    });
}

/// The same with a three-way comparator.
pub fn crt_qsort_by<T>(a: &mut [T], cmp: impl Fn(&T, &T) -> i32) {
    const CUTOFF: usize = 8;
    if a.len() < 2 {
        return;
    }
    let mut stack: Vec<(usize, usize)> = Vec::new();
    let (mut lo, mut hi) = (0usize, a.len() - 1);
    loop {
        let size = hi - lo + 1;
        if size <= CUTOFF {
            // shortsort
            let mut h = hi;
            while h > lo {
                let mut max = lo;
                for p in lo + 1..=h {
                    if cmp(&a[p], &a[max]) > 0 {
                        max = p;
                    }
                }
                a.swap(max, h);
                h -= 1;
            }
        } else {
            let mut mid = lo + size / 2;
            if cmp(&a[lo], &a[mid]) > 0 {
                a.swap(lo, mid);
            }
            if cmp(&a[lo], &a[hi]) > 0 {
                a.swap(lo, hi);
            }
            if cmp(&a[mid], &a[hi]) > 0 {
                a.swap(mid, hi);
            }
            let (mut loguy, mut higuy) = (lo as isize, hi as isize);
            let m = |mid: usize| mid as isize;
            loop {
                if m(mid) > loguy {
                    loop {
                        loguy += 1;
                        if !(loguy < m(mid) && cmp(&a[loguy as usize], &a[mid]) <= 0) {
                            break;
                        }
                    }
                }
                if m(mid) <= loguy {
                    loop {
                        loguy += 1;
                        if !(loguy <= hi as isize && cmp(&a[loguy as usize], &a[mid]) <= 0) {
                            break;
                        }
                    }
                }
                loop {
                    higuy -= 1;
                    if !(higuy > m(mid) && cmp(&a[higuy as usize], &a[mid]) > 0) {
                        break;
                    }
                }
                if higuy < loguy {
                    break;
                }
                a.swap(loguy as usize, higuy as usize);
                if m(mid) == higuy {
                    mid = loguy as usize;
                }
            }
            higuy += 1;
            if m(mid) < higuy {
                loop {
                    higuy -= 1;
                    if !(higuy > m(mid) && cmp(&a[higuy as usize], &a[mid]) == 0) {
                        break;
                    }
                }
            }
            if m(mid) >= higuy {
                loop {
                    higuy -= 1;
                    if !(higuy > lo as isize && cmp(&a[higuy as usize], &a[mid]) == 0) {
                        break;
                    }
                }
            }
            if higuy - lo as isize >= hi as isize - loguy {
                if (lo as isize) < higuy {
                    stack.push((lo, higuy as usize));
                }
                if loguy < hi as isize {
                    lo = loguy as usize;
                    continue;
                }
            } else {
                if loguy < hi as isize {
                    stack.push((loguy as usize, hi));
                }
                if (lo as isize) < higuy {
                    hi = higuy as usize;
                    continue;
                }
            }
        }
        match stack.pop() {
            Some((l, h)) => {
                lo = l;
                hi = h;
            }
            None => return,
        }
    }
}

impl CitizenAi {
    /// `FUN_1401148b0`'s timers: count cooldowns down, advance the loiter and carry clocks.
    pub fn tick(&mut self, dt: i64, carrying: i32) {
        for c in &mut self.cooldowns {
            *c = (*c - dt).max(0);
        }
        self.since_loiter += dt;
        self.carrying_time += dt;
        if carrying == 0 {
            self.carrying_time = 0;
        }
    }

    /// `FUN_1401148b0`: the scheduled update. `busy()` is the character's "has actions" test
    /// (`+0x60 >= 0`); `run` dispatches an activity (see [`CitizenAi::choose`]). Eating from a
    /// stocked home before an urgent choice (`FUN_14011a7b0`) and the per-profession inventory
    /// size and model refresh (`FUN_140114c70`, `FUN_140114d60`) are the caller's.
    pub fn update(
        &mut self,
        dt: i64,
        view: &CitizenView,
        d: &Thresholds,
        rng: &mut Random,
        busy: impl Fn() -> bool,
        mut run: impl FnMut(&mut CitizenAi, u32, &mut Random) -> bool,
    ) {
        self.tick(dt, view.carrying);
        let urgent_running = (self.eating && self.current == EAT_URGENT)
            || (self.warming && self.current == WARM_URGENT);
        let in_urgent = self.current == EAT_URGENT || self.current == WARM_URGENT;
        if view.hunger <= d.hungry_urgent
            || urgent_running
            || in_urgent
            || self.cooldowns[EAT_URGENT as usize] > 0
        {
            if self.eating && view.hunger <= d.hungry_urgent {
                self.eating = false;
            }
        } else {
            self.choose(view, d, true, rng, &mut run);
            self.eating = self.current == EAT_URGENT;
        }
        // `bVar4` is computed once, before the hunger choice.
        let in_urgent = self.current == EAT_URGENT || self.current == WARM_URGENT;
        if view.cold <= d.cold_urgent
            || urgent_running
            || in_urgent
            || self.cooldowns[WARM_URGENT as usize] > 0
        {
            if self.warming && view.cold <= d.cold_urgent {
                self.warming = false;
            }
        } else {
            self.choose(view, d, true, rng, &mut run);
            self.warming = self.current == WARM_URGENT;
        }
        if !self.eating && !self.warming && view.emergency && self.current != EMERGENCY {
            self.choose(view, d, true, rng, &mut run);
        }
        if busy() {
            return;
        }
        self.choose(view, d, false, rng, &mut run);
    }

    /// `FUN_140113060`: run the queued activity, or try candidates in order until `run`
    /// leaves the character busy. `run(activity)` is the dispatcher (`FUN_1401146c0`) and
    /// returns whether the character now has actions queued.
    pub fn choose(
        &mut self,
        view: &CitizenView,
        d: &Thresholds,
        urgent_only: bool,
        rng: &mut Random,
        mut run: impl FnMut(&mut CitizenAi, u32, &mut Random) -> bool,
    ) {
        if let Some(q) = self.queued {
            if self.dispatch(q, &mut run, rng) {
                self.current = q;
                self.queued = None;
                return;
            }
        }
        let (mut list, shuffle) = candidates(self, view, d, urgent_only);
        if shuffle {
            shuffle_and_sort(&mut list, rng);
        }
        for (activity, _) in list {
            if self.dispatch(activity, &mut run, rng) {
                self.current = activity;
                break;
            }
        }
    }

    /// `FUN_1401146c0`: only activities whose cooldown has expired run.
    fn dispatch(
        &mut self,
        activity: u32,
        run: &mut impl FnMut(&mut CitizenAi, u32, &mut Random) -> bool,
        rng: &mut Random,
    ) -> bool {
        if self.cooldowns[activity as usize] >= 1 {
            return false;
        }
        run(self, activity, rng)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qsort_orders_by_priority_descending() {
        let mut v: Vec<Candidate> = (0..20).map(|i| (i, (i as i32 * 7) % 5)).collect();
        crt_qsort(&mut v);
        assert!(v.windows(2).all(|w| w[0].1 >= w[1].1), "{v:?}");
        let mut v: Vec<Candidate> = vec![(12, 0), (8, 5), (3, 3), (9, 2)];
        crt_qsort(&mut v);
        assert_eq!(v, vec![(8, 5), (3, 3), (9, 2), (12, 0)]);
    }

    #[test]
    fn new_adult_candidates() {
        let ai = CitizenAi::default();
        let view = CitizenView {
            age: 20.0,
            health: 1.0,
            ..Default::default()
        };
        let d = Thresholds {
            adult_age: 10.0,
            hungry: 50.0,
            hungry_urgent: 75.0,
            cold: 50.0,
            cold_urgent: 75.0,
            ..Default::default()
        };
        let (c, shuffle) = candidates(&ai, &view, &d, false);
        assert!(shuffle);
        assert_eq!(c, vec![(CLOTHING, 3), (IDLE, 0), (TOOL, 5), (WORK, 2)]);
    }
}
