//! Achievements (Runtime `Achievement::AchievementManager`, `AchievementInstance`, `GoalList`,
//! `GoalInstance`; the goal classes in the Application). See docs/ACHIEVEMENTS.md.
//!
//! `Game/Achievement/AchievementList.rsc` lists the achievements; each has goals. A goal's state
//! (`Goal` slot `+0x40`, `GoalInstance::CheckGoalState`) is 0 not met, 1 met or 2 failed; a
//! failed goal stays failed (`GoalInstance::Update`). The manager (`Update` `0x18009f1e0`)
//! checks one achievement not yet awarded each frame in turn, its goals updated with the time
//! since its last check; with every goal met and none failed it is awarded
//! (`SetAchievementMet`) and the game logs event `0x19` (`FUN_14009be30`).
//!
//! Deviation: the original keeps what is awarded in Steam's user stats; the port keeps it in
//! the settings store (`Achievement<title>`).

use banished_formats::pkg::Archive;
use banished_formats::scenario::{self, GoalKind};
use banished_game::scheduler::ticks;
use banished_game::tracker::Statistics;

use crate::start::StartingTown;

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

/// `GoalInstance`: `+0x10` bit 0 met, bit 1 failed; TimeGoalInstance `+0x20` the time the
/// other goals have held.
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
    /// `Game +0x128`: the year, and the seconds into it.
    pub year: u32,
    pub year_seconds: f32,
    /// The game's terrain and climate (`Game +0x48`, `+0x50`) and size (`Game +0x34`).
    pub terrain: &'a str,
    pub climate: &'a str,
    pub size: i32,
}

/// The list's achievements.
pub fn load(archive: &Archive) -> Vec<Achievement> {
    let read = |r: &str| archive.read_ref(&r.replace('\\', "/")).ok();
    let Some(list) = read("Game/Achievement/AchievementList.rsc:resource")
        .and_then(|d| banished_formats::game::external_list(&d).ok())
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

/// The file part of a reference (`Template\X.rsc:resource` → `template\x.rsc`).
fn file(r: &str) -> String {
    r.split(':').next().unwrap_or("").to_ascii_lowercase()
}

fn same(a: &str, b: &str) -> bool {
    !b.is_empty() && file(a) == file(b)
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

/// The known crops (0), trees (1) or livestock (2) by their SelectDescription (`+0x40`).
fn known_of(archive: Option<&Archive>, town: &StartingTown, kind: i32) -> i32 {
    let Some(archive) = archive else { return 0 };
    town.construction
        .known
        .iter()
        .filter(|&&t| {
            let f = file(&town.templates[t].reference).replace('\\', "/");
            archive
                .read_ref(&format!("{f}:select"))
                .ok()
                .and_then(|d| select_kind(&d))
                == Some(kind)
        })
        .count() as i32
}

/// SelectDescription `+0x40`.
fn select_kind(data: &[u8]) -> Option<i32> {
    use banished_formats::crs;
    crs::parse(data, |r: &mut crs::Reader, _| {
        r.block(|r, _| {
            r.external_ref()?;
            r.string()?;
            r.external_ref()?;
            r.string()?;
            let k = r.i32()?;
            r.skip_rest();
            Ok(k)
        })
    })
    .ok()
    .map(|(_, k)| k)
}

/// `FUN_14009f0` … `FUN_1400df9b0`: a statistic. `flags` is StatGoal `+0x78`, `index` `+0x7c`.
fn stat(
    x: &GoalContext,
    archive: Option<&Archive>,
    kind: i32,
    r: &str,
    flags: u32,
    index: i32,
) -> i32 {
    let town = x.town;
    let rules = &town.rules;
    let education = town.agents.rules().education;
    let live = || {
        town.citizens
            .iter()
            .zip(&town.agents.states)
            .enumerate()
            .filter(|(_, (c, _))| c.alive)
    };
    let working = |c: &banished_game::citizens::Citizen| c.is_adult(rules) && c.school.is_none();
    let material = || town.materials.iter().position(|m| same(&m.reference, r));
    match kind {
        // `FUN_1400df8e0`: educated working adults.
        0 => live()
            .filter(|(_, (c, _))| working(c) && education <= c.education)
            .count() as i32,
        // `FUN_1400df770`: working adults with the material's tool (or one past use).
        1 => {
            let uses = material().map_or(i32::MIN, |m| town.materials[m].tool_uses);
            live()
                .filter(|(_, (c, s))| working(c) && (s.tool_status == uses || s.tool_status < 0))
                .count() as i32
        }
        // `FUN_1400df620`: citizens in a coat of the material's kind (warm, flag `0x8000`).
        2 => {
            let warm = material().is_some_and(|m| town.materials[m].flags & 0x8000 != 0);
            live()
                .filter(|(_, (_, s))| {
                    let v = s.clothing_status;
                    ((v == 2 || v == 4) && warm) || ((v == 1 || v == 4) && !warm)
                })
                .count() as i32
        }
        // `FUN_1400df570`, `FUN_1400df4c0`: the mean happiness and health.
        3 | 4 => {
            let v: Vec<f32> = live()
                .map(|(_, (_, s))| if kind == 3 { s.happiness } else { s.health })
                .collect();
            if v.is_empty() {
                0
            } else {
                (v.iter().sum::<f32>() / v.len() as f32) as i32
            }
        }
        // Stock manager `+0xa8`: the population.
        5 => live().count() as i32,
        6 => live().filter(|(_, (c, _))| working(c)).count() as i32,
        7 => live().filter(|(_, (c, _))| c.school.is_some()).count() as i32,
        8 => live()
            .filter(|(_, (c, _))| !c.is_adult(rules) && c.school.is_none())
            .count() as i32,
        // `FUN_1400df1a0`: working adults of the profession.
        9 => {
            let p = town
                .construction
                .professions
                .list
                .iter()
                .position(|p| p.reference.eq_ignore_ascii_case(r));
            live()
                .filter(|(_, (c, s))| working(c) && Some(s.profession) == p)
                .count() as i32
        }
        // `FUN_1400df150`, `FUN_1400df0e0`: the game's terrain or climate.
        10 => same(x.terrain, r) as i32,
        11 => same(x.climate, r) as i32,
        // `FUN_1400df050`: graves with a tombstone.
        12 => town
            .construction
            .cemeteries
            .iter()
            .filter_map(|&p| town.construction.sites.get(&p))
            .flat_map(|s| s.graves.iter())
            .filter(|&&c| {
                town.cells.objects[c as usize]
                    .is_some_and(|o| town.construction.tombstones.contains_key(&o))
            })
            .count() as i32,
        // Stock manager `+0x1ac`: the value traded away.
        13 => town.construction.trade_value,
        // `FUN_1400deb20`: what the storages, homes and trading posts (of the template) hold
        // of the flags.
        14 => {
            let mut storages: Vec<(usize, usize)> = town
                .storage_sites
                .iter()
                .map(|s| (s.entity, s.storage))
                .collect();
            for (&e, h) in &town.construction.homes {
                storages.push((e, h.storage));
            }
            for &p in &town.construction.trading_posts {
                if let Some(s) = town.construction.sites.get(&p) {
                    storages.push((p, s.storage));
                }
            }
            storages.sort_unstable();
            storages.dedup();
            storages
                .iter()
                .filter(|(e, _)| {
                    r.is_empty() || same(&town.templates[town.placed[*e].template].reference, r)
                })
                .filter_map(|(_, s)| town.storages.get(*s))
                .flat_map(|s| s.items.iter())
                .filter(|i| town.materials[i.material].flags & flags != 0)
                .map(|i| i.count)
                .sum()
        }
        // `FUN_1400dea30`, `FUN_1400de940`, `FUN_1400de850`: known livestock, crops, trees.
        15 => known_of(archive, town, 2),
        16 => known_of(archive, town, 0),
        17 => known_of(archive, town, 1),
        // `FUN_1400de7a0`: the animals in pastures of the kind.
        18 => town
            .wildlife
            .pastures
            .values()
            .filter(|p| {
                p.kind
                    .is_some_and(|k| same(&town.templates[k].reference, r))
            })
            .map(|p| p.livestock.len() as i32)
            .sum(),
        // `FUN_1400de240`: the kinds of the flags this year's workplaces (of the template)
        // made.
        19 => {
            let mut kinds = std::collections::BTreeSet::new();
            for e in workplaces(town, r) {
                if let Some(t) = town.construction.trackers.get(&e) {
                    for &(m, _) in &t.this_year {
                        if town.materials[m].flags & flags != 0 {
                            kinds.insert(m);
                        }
                    }
                }
            }
            kinds.len() as i32
        }
        // `FUN_1400de120`: the stock slot's production over the last `index` years.
        20 => {
            let slot = [
                (0x20000, 8),
                (0x4000, 7),
                (0x40, 3),
                (0x20, 5),
                (0x2000, 6),
                (0x1000, 2),
                (0x800, 1),
                (0x400, 0),
                (0x200, 4),
            ]
            .iter()
            .find(|(f, _)| flags & f != 0)
            .map_or(4, |&(_, s)| s);
            let series = 8 + 3 * slot + 2;
            let into = ((x.year_seconds / 3600.0) * 48.0) as i32;
            let into = if 0x2f < into { 0 } else { into };
            let year = x.year as i32;
            sum(
                x.statistics,
                series,
                (year - index - 1) * 48,
                year * 48 - 48 + into,
            )
        }
        // `FUN_1400dddb0`: what this year's workplaces of the template made of the flags.
        21 => workplaces(town, r)
            .iter()
            .filter_map(|e| town.construction.trackers.get(e))
            .flat_map(|t| t.this_year.iter())
            .filter(|(m, _)| town.materials[*m].flags & flags != 0)
            .map(|(_, n)| n)
            .sum(),
        // `FUN_1400ddbb0`: the workers at the template's workplaces.
        22 => workplaces(town, r)
            .iter()
            .filter_map(|e| town.construction.workforce.workplaces.get(e))
            .map(|w| w.workers.len() as i32)
            .sum(),
        // `FUN_1400ddb70`: the map's built road cells of a kind (`Game +0x110 → +0x220`).
        23 => (0..town.cells.road_kind.len())
            .filter(|&i| {
                town.cells.road_kind[i] as i32 == index && town.cells.road_built[i] == 0xff
            })
            .count() as i32,
        // `FUN_1400ddac0`: the residences (of the template).
        24 => town
            .construction
            .workforce
            .residences
            .iter()
            .filter(|&&e| {
                r.is_empty() || same(&town.templates[town.placed[e].template].reference, r)
            })
            .count() as i32,
        // `FUN_1400dd680`: the template's buildings standing.
        25 => entities(town, r).filter(|&e| standing(town, e)).count() as i32,
        // `Game +0x128`: the year.
        26 => x.year as i32,
        // `Game +0x34`: the map size.
        27 => x.size,
        // Stock manager `+0x1b0`: nomads taken in.
        28 => town.construction.nomads_admitted,
        // `FUN_1400dd1c0`: the longest side of the template's standing buildings.
        29 => entities(town, r)
            .filter(|&e| standing(town, e))
            .map(|e| {
                let f = town.placed[e].footprint;
                f.0.max(f.1)
            })
            .max()
            .unwrap_or(0),
        _ => 0,
    }
}

/// `FUN_1400963f0`: a series' sum over samples `from..=to`, clamped to what exists.
fn sum(s: &Statistics, series: usize, from: i32, to: i32) -> i32 {
    let Some(v) = s.series.get(series) else {
        return 0;
    };
    let from = from.max(0);
    let to = to.min(v.len() as i32 - 1);
    if to < from {
        return 0;
    }
    v[from as usize..=to as usize].iter().sum()
}

/// A goal's state (`Goal` slot `+0x40`; TimeGoalInstance `FUN_1400e0180`).
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
        GoalKind::Time { years, .. } => {
            met((*years * 3600) as f32 <= instance.time as f32 / ticks(1.0) as f32)
        }
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

    /// `GoalList::Update`: each goal in turn (a TimeGoal's time first grows, back to none
    /// unless the others are met, `FUN_1400e0100`), then `GoalInstance::Update`.
    pub fn update(
        &mut self,
        a: &Achievement,
        dt: i64,
        x: &GoalContext,
        archive: Option<&Archive>,
        tutorial_done: impl Fn(usize) -> bool,
    ) {
        for i in 0..self.goals.len() {
            let goal = &a.goals[i];
            if let GoalKind::Time { .. } = goal.kind {
                self.goals[i].time += dt;
                let others = self.goals.iter().enumerate().all(|(j, g)| j == i || g.met);
                if !others {
                    self.goals[i].time = 0;
                }
            }
            if self.goals[i].failed {
                continue;
            }
            let state = match goal.kind {
                GoalKind::FinishTutorial(_) => {
                    if tutorial_done(i) {
                        GoalState::Met
                    } else {
                        GoalState::NotMet
                    }
                }
                _ => check(goal, &self.goals[i], x, archive),
            };
            match state {
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

/// `AchievementManager`: `+0x78` awarded, `+0x88` the instances, `+0xa0` the time since each
/// was last checked, `+0xb0` the next to check.
#[derive(Debug, Clone, Default)]
pub struct AchievementManager {
    pub awarded: Vec<bool>,
    pub instances: Vec<AchievementInstance>,
    pub elapsed: Vec<i64>,
    pub cursor: usize,
}

impl AchievementManager {
    pub fn new(list: &[Achievement], settings: &crate::settings::Persistence) -> Self {
        let awarded = list
            .iter()
            .map(|a| {
                let mut v = false;
                settings.get_bool("Achievement", &a.desc.title, &mut v);
                v
            })
            .collect();
        AchievementManager {
            awarded,
            instances: list.iter().map(AchievementInstance::new).collect(),
            elapsed: vec![0; list.len()],
            cursor: 0,
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
        let n = list.len().min(self.instances.len());
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
        self.instances[i].update(&list[i], elapsed, x, archive, |_| false);
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
