//! Achievements (Runtime `Achievement::AchievementManager`, `AchievementInstance`, `GoalList`,
//! `GoalInstance`; the goal classes in the Application). See docs/ACHIEVEMENTS.md.
//!
//! `Game/Achievement/AchievementList.rsc` lists the achievements; each has goals. A goal's state
//! (`GoalInstance::CheckGoalState`) is 0 not met, 1 met or 2 failed; a failed goal stays failed
//! (`GoalInstance::Update`). The manager's `Update` checks one achievement not yet awarded each
//! frame in turn, its goals updated with the time since its last check; with every goal met and
//! none failed it is awarded (`SetAchievementMet`) and the game logs event `0x19`.
//!
//! Original: `FUN_14009be30`, Runtime `0x18009f1e0`.
//!
//! Deviation: the original keeps what is awarded in Steam's user stats; the port keeps it in
//! the settings store (`Achievement<title>`).

mod persist;
mod stats;
pub use persist::{saved_instance, write_instance};
use stats::stat;

use unbanished_formats::pkg::Archive;
use unbanished_formats::scenario::{self, GoalKind};
use unbanished_game::work::tracker::Statistics;

use crate::town::StartingTown;

/// An achievement and its goals.
#[derive(Debug, Clone)]
pub struct Achievement {
    /// Its reference in the list, and its goals' (saving names them).
    pub reference: String,
    pub goal_references: Vec<String>,
    pub desc: scenario::Achievement,
    pub goals: Vec<scenario::Goal>,
}

/// `GoalState`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalState {
    NotMet = 0,
    Met = 1,
    Failed = 2,
}

// +0x10; +0x20
/// `GoalInstance`: bit 0 met, bit 1 failed; TimeGoalInstance the time the other goals have held.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GoalInstance {
    pub met: bool,
    pub failed: bool,
    pub time: i64,
}

/// What a goal reads besides the town.
pub struct GoalContext<'a> {
    pub town: &'a StartingTown,
    pub statistics: &'a Statistics,
    // Game +0x128
    /// `Game`: the year, and the seconds into it.
    pub year: u32,
    pub year_seconds: f32,
    // +0x50; Game +0x48; Game +0x34
    /// The game's terrain and climate (`Game`) and size (`Game`).
    pub terrain: &'a str,
    pub climate: &'a str,
    pub size: i32,
}

/// The list's achievements.
pub fn load(archive: &Archive) -> Vec<Achievement> {
    let read = |r: &str| archive.read_ref(&r.replace('\\', "/")).ok();
    let Some(list) = read("Game/Achievement/AchievementList.rsc:resource")
        .and_then(|d| unbanished_formats::game::external_list(&d).ok())
    else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|r| {
            let desc = scenario::achievement_description(&read(r)?).ok()?;
            let (goal_references, goals) = desc
                .goals
                .iter()
                .flatten()
                .filter_map(|g| Some((g.clone(), scenario::goal(&read(g)?).ok()?)))
                .unzip();
            Some(Achievement {
                reference: r.clone(),
                goal_references,
                desc,
                goals,
            })
        })
        .collect()
}

/// The file part of a reference (`Template\X.rsc:resource` → `Template\X.rsc`).
fn file(r: &str) -> &str {
    r.split(':').next().unwrap_or("")
}

/// Whether two references name the same file, ignoring ASCII case. Compares in place: this
/// runs for every entity and goal each frame, and lowercased copies were a large share of the
/// frame time.
fn same(a: &str, b: &str) -> bool {
    !b.is_empty() && file(a).eq_ignore_ascii_case(file(b))
}

/// Whether two references name the same object (the case and slashes aside), without copying.
pub fn same_reference(a: &str, b: &str) -> bool {
    let norm = |c: u8| {
        if c == b'/' {
            b'\\'
        } else {
            c.to_ascii_lowercase()
        }
    };
    !b.is_empty() && a.len() == b.len() && a.bytes().zip(b.bytes()).all(|(x, y)| norm(x) == norm(y))
}

/// The live entities of the template `r` (the stock manager's `+0x160` by template).
fn entities<'a>(town: &'a StartingTown, r: &'a str) -> impl Iterator<Item = usize> + 'a {
    town.placed
        .iter()
        .enumerate()
        .filter(move |(_, p)| p.alive && same(&town.templates[p.template].reference, r))
        .map(|(i, _)| i)
}

/// `FUN_1400dd680`'s test: built (its BuildComponent's stages done, or a site complete) and
/// neither torn down (states 6, 7) nor being removed.
fn standing(town: &StartingTown, e: usize) -> bool {
    match town.construction.sites.get(&e) {
        Some(s) => {
            s.complete && !matches!(s.machine.state, 6 | 7) && !town.placed[e].removal_requested
        }
        None => !town.placed[e].removal_requested,
    }
}

/// The workplaces of every profession whose template is `r` (or any).
fn workplaces<'a>(town: &'a StartingTown, r: &'a str) -> Vec<usize> {
    let mut out = Vec::new();
    for list in town.construction.workforce.registry.values() {
        for &e in list {
            if r.is_empty() || same(&town.templates[town.placed[e].template].reference, r) {
                out.push(e);
            }
        }
    }
    out
}

/// The years (`× 3600` s) have passed, the time through `Time::AsSeconds`.
///
/// Original: `FUN_1400e0180`.
pub fn time_goal_met(years: i32, time: i64) -> bool {
    (years * 3600) as f32 <= unbanished_game::scheduler::seconds(time)
}

// Goal slot +0x40
/// A goal's state (`CheckGoalState`; a time goal's comes from its time).
///
/// Original: `FUN_1400e0180`.
pub fn check(
    goal: &scenario::Goal,
    instance: &GoalInstance,
    x: &GoalContext,
    archive: Option<&Archive>,
) -> GoalState {
    let met = |b: bool| if b { GoalState::Met } else { GoalState::NotMet };
    match &goal.kind {
        // `FUN_1400dca50`: all of the buildings, or any of them.
        GoalKind::Building { all, buildings } => {
            let have = buildings
                .iter()
                .filter(|b| entities(x.town, b).next().is_some())
                .count();
            met(if *all {
                have == buildings.len()
            } else {
                0 < have
            })
        }
        // `FUN_1400dccf0`.
        GoalKind::Population(n) => met(*n <= stat(x, archive, 5, "", 0, 0)),
        // `FUN_1400dfbf0`: the first statistic against the amount, or (odd comparisons) the
        // second; with `flag` the goal fails when it holds and is met until then.
        GoalKind::Stat {
            e30,
            e34,
            e38,
            amount,
            flag,
            r48,
            r60,
            e78,
            i7c,
        } => {
            let a = stat(x, archive, *e30, r48, *e78 as u32, *i7c);
            let b = if e38 & 1 == 0 {
                *amount
            } else {
                stat(x, archive, *e34, r60, *e78 as u32, *i7c)
            };
            let holds = match e38 {
                0 | 1 => a == b,
                2 | 3 => a != b,
                4 | 5 => a < b,
                6 | 7 => a <= b,
                8 | 9 => b < a,
                10 | 11 => b <= a,
                _ => false,
            };
            match (*flag, holds) {
                (false, h) => met(h),
                (true, false) => GoalState::Met,
                (true, true) => GoalState::Failed,
            }
        }
        // `FUN_1400e0180`: the years have passed while the other goals held.
        GoalKind::Time { years, .. } => met(time_goal_met(*years, instance.time)),
        // `FUN_1400dd080`: the tutorial is done (its instance tracks it).
        GoalKind::FinishTutorial(_) => GoalState::NotMet,
    }
}

/// `AchievementInstance`: its goals' instances.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AchievementInstance {
    pub goals: Vec<GoalInstance>,
}

impl AchievementInstance {
    pub fn new(a: &Achievement) -> Self {
        AchievementInstance {
            goals: vec![GoalInstance::default(); a.goals.len()],
        }
    }

    /// Each goal in turn (a TimeGoal's time first grows, back to none unless the others are met),
    /// then `GoalInstance::Update`.
    ///
    /// Original: `GoalList::Update`, `FUN_1400e0100`.
    pub fn update(
        &mut self,
        a: &Achievement,
        dt: i64,
        x: &GoalContext,
        archive: Option<&Archive>,
        tutorial_done: impl Fn(usize) -> bool,
    ) {
        let timed = |i: usize| matches!(a.goals[i].kind, GoalKind::Time { .. });
        self.update_with(timed, dt, |i, g| match a.goals[i].kind {
            GoalKind::FinishTutorial(_) => {
                if tutorial_done(i) {
                    GoalState::Met
                } else {
                    GoalState::NotMet
                }
            }
            _ => check(&a.goals[i], g, x, archive),
        });
    }

    /// `GoalList::Update` over goal states from `state` (`CheckGoalState`); `timed` tells the
    /// TimeGoals.
    pub fn update_with(
        &mut self,
        timed: impl Fn(usize) -> bool,
        dt: i64,
        mut state: impl FnMut(usize, &GoalInstance) -> GoalState,
    ) {
        for i in 0..self.goals.len() {
            if timed(i) {
                self.goals[i].time += dt;
                let others = self.goals.iter().enumerate().all(|(j, g)| j == i || g.met);
                if !others {
                    self.goals[i].time = 0;
                }
            }
            if self.goals[i].failed {
                continue;
            }
            match state(i, &self.goals[i]) {
                GoalState::Met => self.goals[i].met = true,
                GoalState::NotMet => self.goals[i].met = false,
                GoalState::Failed => self.goals[i].failed = true,
            }
        }
    }

    /// `AchievementInstance::IsMet`: goals, all met, none failed.
    pub fn is_met(&self) -> bool {
        !self.goals.is_empty() && self.goals.iter().all(|g| g.met && !g.failed)
    }

    #[allow(dead_code)]
    pub fn has_failed(&self) -> bool {
        self.goals.iter().any(|g| g.failed)
    }
}

// +0x78; +0x88; +0xa0; +0xb0
/// `AchievementManager`: awarded, the instances, the time since each was last checked, the next to
/// check.
#[derive(Debug, Clone, Default)]
pub struct AchievementManager {
    pub awarded: Vec<bool>,
    pub instances: Vec<AchievementInstance>,
    pub elapsed: Vec<i64>,
    pub cursor: usize,
}

impl AchievementManager {
    /// A manager for `list`; `awarded` says whether the achievement with this title was
    /// awarded before (the game app reads its settings store).
    pub fn new(list: &[Achievement], awarded: impl Fn(&str) -> bool) -> Self {
        let awarded = list.iter().map(|a| awarded(&a.desc.title)).collect();
        AchievementManager {
            awarded,
            instances: list.iter().map(AchievementInstance::new).collect(),
            elapsed: vec![0; list.len()],
            cursor: 0,
        }
    }

    /// `AchievementManager::Persist` on load: the saved instances replace the new ones, each
    /// by its achievement's reference.
    pub fn restore(&mut self, list: &[Achievement], saved: &crate::saved::SavedGame) {
        use unbanished_formats::savegame::Value;
        let Some(Value::List(items)) = saved.state.achievements.at("achievements.instances") else {
            return;
        };
        for item in items {
            let Some(id) = item.ptr("instance") else {
                continue;
            };
            let Some(o) = saved.object(id) else { continue };
            let reference = saved.ext(&o.fields, "instance.achievement").unwrap_or("");
            let Some(i) = list
                .iter()
                .position(|a| same_reference(&a.reference, reference))
            else {
                continue;
            };
            if let (Some(slot), Some(x)) = (
                self.instances.get_mut(i),
                saved_instance(saved, id, &list[i]),
            ) {
                *slot = x;
            }
        }
    }

    /// `AchievementManager::Update`: the time grows for those not yet awarded; the next one
    /// not awarded is checked with it, and awarded once all its goals are met. Returns the
    /// one awarded.
    pub fn update(
        &mut self,
        list: &[Achievement],
        dt: i64,
        x: &GoalContext,
        archive: Option<&Archive>,
    ) -> Option<usize> {
        self.update_with(list.len(), dt, |i, inst, elapsed| {
            inst.update(&list[i], elapsed, x, archive, |_| false)
        })
    }

    /// `Update` over `n` achievements, `visit` updating the one checked with its time.
    pub fn update_with(
        &mut self,
        n: usize,
        dt: i64,
        mut visit: impl FnMut(usize, &mut AchievementInstance, i64),
    ) -> Option<usize> {
        let n = n.min(self.instances.len());
        if n == 0 {
            return None;
        }
        for i in 0..n {
            if self.awarded[i] {
                self.elapsed[i] = 0;
            } else {
                self.elapsed[i] += dt;
            }
        }
        let mut tried = 0;
        while self.awarded[self.cursor] {
            if n <= tried {
                break;
            }
            tried += 1;
            self.cursor = (self.cursor + 1) % n;
        }
        if tried == n {
            return None;
        }
        let i = self.cursor;
        let elapsed = self.elapsed[i];
        visit(i, &mut self.instances[i], elapsed);
        self.elapsed[i] = 0;
        let mut won = None;
        if self.instances[i].is_met() && !self.awarded[i] {
            self.awarded[i] = true;
            won = Some(i);
        }
        self.cursor = (self.cursor + 1) % n;
        won
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;
