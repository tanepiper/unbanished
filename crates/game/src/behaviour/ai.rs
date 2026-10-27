//! CitizenAIComponent: choosing what a citizen does next. See docs/CITIZEN-AI.md.
//!
//! Original: `FUN_140115040`.
mod qsort;
pub use qsort::{crt_qsort, crt_qsort_by, crt_qsort_less};

// vtable 0x140168890

use crate::random::Random;
use crate::scheduler::{seconds, ticks};

/// Activities.
///
/// Original: `FUN_1401146c0`.
pub mod activity {
    /// Eat.
    pub const EAT: u32 = 0;
    /// Get warm.
    pub const WARM: u32 = 1;
    /// Emergency work, for example fighting a fire.
    pub const EMERGENCY: u32 = 2;
    /// Fetch clothing.
    pub const CLOTHING: u32 = 3;
    /// Fetch herbs.
    pub const HERBS: u32 = 4;
    /// Visit the doctor.
    pub const DOCTOR: u32 = 5;
    /// Stock the home with food.
    pub const HOME_FOOD: u32 = 6;
    /// Stock the home with firewood.
    pub const HOME_FUEL: u32 = 7;
    /// Fetch a tool.
    pub const TOOL: u32 = 8;
    /// Work.
    pub const WORK: u32 = 9;
    /// Go to school.
    pub const SCHOOL: u32 = 10;
    /// A child plays near its guardian.
    pub const PLAY: u32 = 11;
    /// Loiter near a building, then wait.
    pub const IDLE: u32 = 12;
    /// Drop off what the citizen carries.
    pub const DROP_OFF: u32 = 13;
    /// Eat, when very hungry.
    pub const EAT_URGENT: u32 = 14;
    /// Get warm, when very cold.
    pub const WARM_URGENT: u32 = 15;
    // FUN_140114c70 (the carried storage's size), FUN_140114d60 (the model).
    /// Not an activity: `run` is asked to refresh the citizen before the normal choice (the
    /// carried storage's size from the profession, and the model).
    pub const REFRESH: u32 = u32::MAX;
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
    // +0x68; +0x228
    /// Whether it has a disease.
    pub sick: bool,
    /// Whether a doctor is assigned.
    pub doctor: bool,
    // +0x44; +0x48; +0x50; +0x54; +0x80
    /// The hunger.
    pub hunger: f32,
    /// The cold.
    pub cold: f32,
    /// The health.
    pub health: f32,
    /// Bit 9 of the health events: herbs are not wanted (*inferred*).
    pub health_flag: bool,
    /// The happiness.
    pub happiness: f32,
    // +0x3c; +0x208; +0x238; +0x1e4; +0xac
    /// The age.
    pub age: f32,
    /// Whether it attends school.
    pub school: bool,
    /// Whether it has a home.
    pub home: bool,
    /// The clothing level.
    pub clothing: i32,
    /// The uses left on its tool.
    pub tool: i32,
    // +0x22
    /// Map byte set at home, work or here.
    ///
    /// Original: `FUN_14011b0b0`.
    pub emergency: bool,
    /// The home has food.
    ///
    /// Original: `FUN_14011abb0`, `FUN_14011acc0`.
    pub home_food: bool,
    /// The home has firewood.
    pub home_fuel: bool,
    // +0x250; +0xb0
    /// The spouse's supply errand: 1 food, 2 firewood.
    pub spouse_activity: Option<u32>,
    /// Carried inventory count.
    ///
    /// Original: `FUN_14011b2b0`.
    pub carrying: i32,
}

/// The CitizenDescription thresholds candidate building reads.
#[derive(Debug, Clone, Copy, Default)]
pub struct Thresholds {
    // +0x44
    /// The age at which a citizen is an adult.
    pub adult_age: f32,
    // +0x74
    /// The health below which a citizen looks for herbs.
    pub health_low: f32,
    // +0xb4
    /// The happiness below which a citizen gets restless after `LOITER_SAD` seconds.
    pub happy_low: f32,
    // +0xb8
    /// The happiness below which a citizen gets restless after `LOITER_UNHAPPY` seconds.
    pub happy_unhappy: f32,
    // +0xc8
    /// The hunger above which a citizen eats.
    pub hungry: f32,
    // +0xcc
    /// The hunger above which a citizen eats at once.
    pub hungry_urgent: f32,
    // +0xe8
    /// The cold above which a citizen warms up.
    pub cold: f32,
    // +0xec
    /// The cold above which a citizen warms up at once.
    pub cold_urgent: f32,
}

/// One candidate (12-byte entries: activity, priority, unused).
pub type Candidate = (u32, i32);

/// A citizen's AI: the cooldown of each activity, the current and queued activity, and the urgent
/// flags.
#[derive(Debug, Clone)]
pub struct CitizenAi {
    // +0x28; +0x2c
    /// The current activity.
    pub current: u32,
    /// The queued activity, if any.
    pub queued: Option<u32>,
    // +0x30 / +0x31
    /// Eating urgently.
    pub eating: bool,
    /// Warming up urgently.
    pub warming: bool,
    // +0x40..+0xb8
    /// Per-activity cooldowns (ticks).
    pub cooldowns: [i64; 16],
    // +0xc0; +0xc8
    /// The time since the citizen last loitered.
    pub since_loiter: i64,
    /// How long the citizen has carried goods (reset while it carries nothing).
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

/// Candidate activities in push order, and whether the list is shuffled and sorted (the doctor-only
/// and urgent-only lists return before that).
///
/// Original: `FUN_140113170`.
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

/// Swap each position with `Rand(count)`, then sort.
///
/// Original: `FUN_140113170` (tail).
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

/// `value <= limit` as a `comiss` with `jbe` (or a `ja` not taken) decides it: true for a NaN.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn at_most(value: f32, limit: f32) -> bool {
    !(limit < value)
}

impl CitizenAi {
    /// Count cooldowns down, advance the loiter and carry clocks.
    ///
    /// Original: `FUN_1401148b0` (timers).
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

    /// Whether this update's hunger branch is taken: urgently hungry, no urgent activity running or
    /// current, and the urgent meal's cooldown out after this update's `dt`. The caller then eats
    /// from the home's storage first when it holds food ([`CitizenAi::update`]'s `home_meal`).
    ///
    /// Original: `FUN_1401148b0`.
    pub fn hunger_branch(&self, dt: i64, view: &CitizenView, d: &Thresholds) -> bool {
        let urgent_running = (self.eating && self.current == EAT_URGENT)
            || (self.warming && self.current == WARM_URGENT);
        let in_urgent = self.current == EAT_URGENT || self.current == WARM_URGENT;
        let cooldown = (self.cooldowns[EAT_URGENT as usize] - dt).max(0);
        // jbe at 0x140114a1b: a NaN hunger is not urgent.
        !(at_most(view.hunger, d.hungry_urgent) || urgent_running || in_urgent || cooldown > 0)
    }

    // +0x60 >= 0
    /// The scheduled update. `busy` is the character's "has actions" test; `run` dispatches an
    /// activity (see [`CitizenAi::choose`]). `home_meal` is the citizen's hunger after eating from
    /// the home's storage, when [`CitizenAi::hunger_branch`] held and the home had food: the urgent
    /// choice is then made only if the citizen is still urgently hungry. Before the normal choice,
    /// `run` is asked for [`activity::REFRESH`] (the carried storage's size and the model).
    ///
    /// Original: `FUN_14011a7b0`, `FUN_140114c70`, `FUN_140114d60`, `FUN_1401148b0`.
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        dt: i64,
        view: &CitizenView,
        d: &Thresholds,
        rng: &mut Random,
        home_meal: Option<f32>,
        busy: impl Fn() -> bool,
        mut run: impl FnMut(&mut CitizenAi, u32, &mut Random) -> bool,
    ) {
        self.tick(dt, view.carrying);
        let urgent_running = (self.eating && self.current == EAT_URGENT)
            || (self.warming && self.current == WARM_URGENT);
        let in_urgent = self.current == EAT_URGENT || self.current == WARM_URGENT;
        // jbe at 0x140114a1b: a NaN hunger is not urgent.
        if at_most(view.hunger, d.hungry_urgent)
            || urgent_running
            || in_urgent
            || self.cooldowns[EAT_URGENT as usize] > 0
        {
            // ja at 0x140114b49: a NaN hunger ends the urgent meal.
            if self.eating && at_most(view.hunger, d.hungry_urgent) {
                self.eating = false;
            }
        } else {
            // `FUN_14011a7b0` ate from the home's storage first; still urgently hungry?
            let hunger = home_meal.unwrap_or(view.hunger);
            if d.hungry_urgent < hunger {
                let fed = CitizenView { hunger, ..*view };
                self.choose(&fed, d, true, rng, &mut run);
                self.eating = self.current == EAT_URGENT;
            }
        }
        // The rest reads the citizen's hunger as it is now, after any meal at home.
        let fed = CitizenView {
            hunger: home_meal.unwrap_or(view.hunger),
            ..*view
        };
        let view = &fed;
        // `bVar4` is computed once, before the hunger choice.
        let in_urgent = self.current == EAT_URGENT || self.current == WARM_URGENT;
        // jbe at 0x140114b75: a NaN cold is not urgent.
        if at_most(view.cold, d.cold_urgent)
            || urgent_running
            || in_urgent
            || self.cooldowns[WARM_URGENT as usize] > 0
        {
            // ja at 0x140114bce: a NaN cold ends the urgent warming.
            if self.warming && at_most(view.cold, d.cold_urgent) {
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
        // `FUN_140114c70` and `FUN_140114d60`, then the normal choice.
        run(self, REFRESH, rng);
        self.choose(view, d, false, rng, &mut run);
    }

    /// Run the queued activity, or try candidates in order until `run` leaves the character busy.
    /// `run(activity)` is the dispatcher and returns whether the character now has actions queued.
    ///
    /// Original: `FUN_1401146c0`, `FUN_140113060`.
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
mod tests;
