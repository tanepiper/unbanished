//! Scenarios: the game's goal, starting a scenario's game, and checking its goal as the town runs.

use super::*;

/// The game's goal (`Game +0x150`, `+0x158` once it is met or failed): the scenario's
/// achievement.
#[derive(Debug, Clone)]
pub struct GameGoal {
    pub achievement: crate::achievements::Achievement,
    pub instance: crate::achievements::AchievementInstance,
    pub done: bool,
}

/// `Game\Scenario\ScenarioList.rsc`.
pub fn scenarios(archive: &Archive) -> Vec<(String, scenario::Scenario)> {
    let read = |r: &str| archive.read_ref(&r.replace('\\', "/")).ok();
    let Some(list) = read("Game/Scenario/ScenarioList.rsc:resource")
        .and_then(|d| unbanished_formats::game::external_list(&d).ok())
    else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|r| Some((r.clone(), scenario::scenario(&read(r)?).ok()?)))
        .collect()
}

/// `FUN_14007d180`: the game for scenario `index`. A scenario naming a save (`+0x88`, the
/// tutorials') loads `Save\{name}.sav` from the data (`SaveLoad` operation 3) and starts it
/// with the scenario (`FUN_1400b9ec0`); the others start a new game ([`scenario_params`]).
///
/// The loop's scenario (`+0x15c`) is the index given: `FUN_14009bfd0` keeps it over the save's
/// when it is not −1.
pub fn scenario_game(
    ui: &mut Ui,
    archive: &unbanished_formats::pkg::Archive,
    global: Option<&crate::global::GlobalData>,
    s: &scenario::Scenario,
    index: usize,
) -> GameplayLoop {
    if !s.save.is_empty() {
        let name = format!("Save\\{}.sav", s.save);
        match archive
            .read(&name)
            .and_then(|b| crate::saves::saved::SavedGame::parse(&b))
        {
            Ok(mut saved) => {
                saved.state.scenario = index as i32;
                let mut g = GameplayLoop::from_save(saved);
                g.scenario_start = true;
                return g;
            }
            Err(e) => log::error!("{name}: {e}"),
        }
    }
    GameplayLoop::new(scenario_params(ui, global, s, index))
}

/// `FUN_1400b9a10`'s new game for a scenario: its seed (`+0x60`), size (`+0x64`), town name
/// (`+0x58`), disasters (`+0x68`), start condition (`+0x70`), terrain (`+0x78`) and climate
/// (`+0x80`).
pub fn scenario_params(
    ui: &mut Ui,
    global: Option<&crate::global::GlobalData>,
    s: &scenario::Scenario,
    index: usize,
) -> crate::view::world::WorldParams {
    let stem = |r: &Option<String>| {
        let r = r.as_deref().unwrap_or("");
        let path = r.split(':').next().unwrap_or(r);
        path.rsplit(['\\', '/'])
            .next()
            .unwrap_or(path)
            .trim_end_matches(".rsc")
            .to_string()
    };
    let same = |a: &str, b: &Option<String>| {
        b.as_deref().is_some_and(|b| {
            a.split(':')
                .next()
                .unwrap_or("")
                .eq_ignore_ascii_case(b.split(':').next().unwrap_or(""))
        })
    };
    let climate = global.and_then(|g| g.climates.iter().find(|c| same(&c.reference, &s.r80)));
    let condition = global.and_then(|g| {
        g.start_conditions
            .iter()
            .find(|c| same(&c.reference, &s.r70))
    });
    let defaults = crate::view::world::WorldParams::default();
    crate::view::world::WorldParams {
        town_name: s.town_name.clone(),
        terrain: stem(&s.r78),
        climate: climate.map_or_else(
            || stem(&s.r80),
            |c| ui.res.string(c.label.0.as_deref(), &c.label.1),
        ),
        climate_ref: climate.map_or(defaults.climate_ref, |c| c.reference.clone()),
        start_condition: condition.map_or_else(
            || stem(&s.r70),
            |c| ui.res.string(c.data.list.as_deref(), &c.data.name),
        ),
        start_condition_ref: condition.map_or_else(
            || s.r70.clone().unwrap_or_default(),
            |c| c.reference.clone(),
        ),
        size: s.i64.max(0) as usize,
        seed: s.i60 as u32,
        disasters: s.b68,
        scenario: Some(index),
    }
}

impl GameplayLoop {
    /// The scenario's goal and, with a FinishTutorialGoal, its tutorial (`FUN_1400dcf60`).
    ///
    /// `FUN_14009bfd0`: a scenario's start (the loop given its index) makes a new goal
    /// instance; a saved game loaded otherwise keeps its saved one (`Game +0x150`) and whether
    /// it was met (`+0x158`).
    pub(crate) fn start_scenario(&mut self, ctx: &mut Ctx) {
        let Some(i) = self.params.scenario else {
            return;
        };
        let Some((reference, s)) = scenarios(ctx.archive).into_iter().nth(i) else {
            return;
        };
        let read = |r: &str| ctx.archive.read_ref(&r.replace('\\', "/")).ok();
        let (goal_references, goals): (Vec<String>, Vec<scenario::Goal>) = s
            .achievement
            .goals
            .iter()
            .flatten()
            .filter_map(|g| Some((g.clone(), scenario::goal(&read(g)?).ok()?)))
            .unzip();
        let tutorial = goals.iter().find_map(|g| match &g.kind {
            scenario::GoalKind::FinishTutorial(Some(t)) => Some(t.clone()),
            _ => None,
        });
        let achievement = crate::achievements::Achievement {
            reference,
            goal_references,
            desc: s.achievement.clone(),
            goals,
        };
        let restored = match (&self.saved, self.scenario_start) {
            (Some(saved), false) => saved.state.goal.map(|id| {
                let instance = crate::achievements::saved_instance(saved, id, &achievement);
                (instance, saved.state.goal_done)
            }),
            _ => None,
        };
        let (instance, done) = match restored {
            Some((Some(instance), done)) => (instance, done),
            _ => (
                crate::achievements::AchievementInstance::new(&achievement),
                false,
            ),
        };
        self.goal = Some(GameGoal {
            instance,
            achievement,
            done,
        });
        if let Some(def) = tutorial.and_then(|t| load(ctx.archive, &t)) {
            // `Game +0xd0 |= 0x5f0`: no disasters, no saving.
            if let Some(town) = &mut self.sim.town {
                town.construction.game_flags |= 0x5f0;
            }
            if let Some(env) = &mut self.sim.environment {
                env.game_flags |= 0x5f0;
            }
            self.start_tutorial(ctx, def);
        }
    }

    /// `FUN_14009b850`'s goal: the tutorial's step machine (its goal instance's update), the
    /// goals, and once met or failed event `0x1a` / `0x1b` with the title (`FUN_14009bd00`).
    pub(crate) fn update_goal(&mut self, ctx: &mut Ctx, dt: f32, sim_dt: i64) {
        self.update_tutorial(ctx, dt);
        let done = self.tutorial.as_ref().is_some_and(|t| t.done());
        let Some(town) = &self.sim.town else { return };
        let Some(goal) = self.goal.as_mut().filter(|g| !g.done) else {
            return;
        };
        let terrain = format!(r"Game\Terrain\{}.rsc", self.params.terrain);
        let x = crate::achievements::GoalContext {
            town,
            statistics: &self.sim.statistics,
            year: self.sim.calendar.year,
            year_seconds: self.sim.calendar.seconds(),
            terrain: &terrain,
            climate: &self.params.climate_ref,
            size: self.params.size as i32,
        };
        goal.instance
            .update(&goal.achievement, sim_dt, &x, Some(ctx.archive), |_| done);
        let met = goal.instance.is_met();
        if !met && !goal.instance.has_failed() {
            return;
        }
        goal.done = true;
        let d = &goal.achievement.desc;
        let title = ctx.ui.res.string(d.strings.as_deref(), &d.title);
        let kind = if met { 0x1a } else { 0x1b };
        let (month, year) = (self.month(), self.sim.calendar.year as i32);
        self.sim.events.add(
            kind,
            4,
            -1,
            month,
            year,
            [title, String::new(), String::new()],
            false,
        );
    }
}
