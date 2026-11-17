//! The game's deterministic state beside the town: the map's heights, the calendar, the shared
//! random stream, the environment, the stock warnings, the statistics and the event log. The
//! game app draws it and plays its sounds; [`Simulation::iterate`] runs one iteration.

use std::collections::BTreeMap;

use unbanished_formats::game::EnvironmentMonth;
use unbanished_formats::names::NameDatabase;
use unbanished_game::calendar::Calendar;
use unbanished_game::events::{EventLog, StockWarnings};
use unbanished_game::nature::environment::{Environment, YEAR_SECONDS};
use unbanished_game::random::Random;
use unbanished_game::terrain::Heights;
use unbanished_game::work::tracker::Statistics;

use crate::achievements::{Achievement, AchievementManager};
use crate::tools::removal_tool::RemovalTool;
use crate::town::StartingTown;

mod achieve;
mod frame;
mod load;
mod log;
mod save;
mod step;

pub use frame::FrameEvents;
pub use step::StepEvents;

const MONTHS_PER_YEAR: usize = 12;

/// The game's deterministic state. The town and the heights are `None` until a game has loaded.
pub struct Simulation {
    /// The map's heights, which the simulation reads and changes (tunnels, flattening).
    pub heights: Option<Heights>,
    pub town: Option<StartingTown>,
    // Game/Game +0x128
    /// The calendar: the year count and the time into the year.
    pub calendar: Calendar,
    // Game/Game +0xd8
    /// The game's shared random stream, `Random(seed)`.
    ///
    /// Original: `FUN_14009bfd0`.
    pub rng: Random,
    pub environment: Option<Environment>,
    pub environment_months: Vec<EnvironmentMonth>,
    // EnvironmentDescription +0x20
    /// What forms while it rains (`Template\Tornado.rsc`).
    pub environment_template: Option<String>,
    // Game +0x138
    /// The event log.
    pub events: EventLog,
    /// Each player's stock: the counts, the previous counts, the limits and the warnings, by
    /// player number. One entry in a single-player game.
    pub stock_warnings: Vec<StockWarnings>,
    // StockManager +0x150
    /// The Town Info graphs' history, sampled every 75 s.
    pub statistics: Statistics,
    /// Updates begun since the game loaded (recordings and replays count by it).
    pub updates: u64,
    /// The player whose command [`Simulation::execute`] is carrying out: 0 in a single-player
    /// game, the command's giver in a multiplayer one. A separate towns game gives that
    /// player's town (step 7 of docs/MULTIPLAYER-PLAN.md).
    pub acting_player: u8,
    /// The player this computer is: whose stock, town hall and windows the interface shows.
    /// 0 outside lockstep.
    pub local_player: u8,
    /// The drag tools commands have started, by template, for `PlayerCommand::Drag`.
    pub drag_tools: BTreeMap<String, RemovalTool>,
    /// The town names the Map window draws from, read the first time they are wanted.
    pub town_names: Option<NameDatabase>,
    /// The achievements (`Game/Achievement/AchievementList.rsc`) and their progress.
    pub achievements: Vec<Achievement>,
    pub achievement_manager: AchievementManager,
    /// A mod's rule hooks, run after each iteration; none in the original game.
    pub hooks: Option<Box<dyn crate::hooks::RuleHooks>>,
}

impl Simulation {
    /// A simulation with no town yet, its random stream started from `seed`.
    pub fn new(seed: u32) -> Simulation {
        Simulation {
            heights: None,
            town: None,
            calendar: Calendar::default(),
            rng: Random::new(seed),
            environment: None,
            environment_months: Vec::new(),
            environment_template: None,
            events: Default::default(),
            stock_warnings: vec![StockWarnings::default()],
            statistics: Default::default(),
            updates: 0,
            acting_player: 0,
            local_player: 0,
            drag_tools: BTreeMap::new(),
            town_names: None,
            achievements: Vec::new(),
            achievement_manager: Default::default(),
            hooks: None,
        }
    }

    /// Takes a save's random stream, calendar, stock warnings, event log and statistics.
    pub fn restore(&mut self, saved: &crate::saved::SavedGame) {
        let s = &saved.state;
        if let Ok(state) = s.random_state.as_slice().try_into() {
            self.rng = Random::from_parts(s.random_index, state);
        }
        self.calendar.year = s.managers.i32("calendar.value").unwrap_or(0).max(0) as u32;
        if let Some(unbanished_formats::savegame::Value::I64(t)) = s.managers.at("calendar.time") {
            self.calendar.elapsed = *t;
        }
        self.stock_warnings[0] = saved.stock_warnings();
        if let Some(owners) = &saved.owners {
            // A separate towns game keeps each player's stock limits beside its save.
            self.set_players(owners.players);
            for (warnings, limits) in self.stock_warnings.iter_mut().zip(&owners.limits) {
                warnings.limits = *limits;
            }
        }
        self.events = saved.event_log();
        self.statistics = saved.statistics();
    }

    /// How many players the game has: one in a single-player game, each with their own stock
    /// limits and counts. A separate towns game sets it from its lobby.
    pub fn set_players(&mut self, players: u8) {
        self.stock_warnings
            .resize(usize::from(players).max(1), StockWarnings::default());
    }

    /// The number of players, one in a single-player game.
    pub fn players(&self) -> u8 {
        self.stock_warnings.len() as u8
    }

    /// The game's ownership and stock limits, for a save's sidecar
    /// ([`crate::owners::Owners`]).
    pub fn owners(&self) -> crate::owners::Owners {
        let town = self.town.as_ref();
        crate::owners::Owners {
            players: self.players(),
            placed: town
                .map(|t| t.placed.iter().map(|p| p.owner).collect())
                .unwrap_or_default(),
            citizens: town
                .map(|t| t.citizens.iter().map(|c| c.owner).collect())
                .unwrap_or_default(),
            limits: self.stock_warnings.iter().map(|w| w.limits).collect(),
        }
    }

    /// The month: `floor(elapsed / 3600 × 12)`.
    pub fn month(&self) -> usize {
        ((self.calendar.seconds() / YEAR_SECONDS * MONTHS_PER_YEAR as f32) as usize)
            .min(MONTHS_PER_YEAR - 1)
    }

    /// The phase after the frame's iterations, run once even at speed zero (the town's
    /// `smooth`). Returns whether the model instances changed.
    ///
    /// Original: `FUN_14009b850`.
    pub fn smooth(&mut self) -> bool {
        let (Some(town), Some(heights)) = (&mut self.town, &self.heights) else {
            return false;
        };
        let _p = crate::timing::scope("smooth");
        town.smooth(&heights.grid(), &mut self.rng)
    }

    /// Applies the town's queued player commands and placed sites, with the calendar and
    /// weather they read.
    pub fn apply_commands(&mut self) {
        let (Some(town), Some(heights)) = (&mut self.town, &self.heights) else {
            return;
        };
        let grid = heights.grid();
        // The crop fields read the calendar (`Game +0x128`).
        town.construction.calendar = self.calendar.seconds();
        town.construction.year = self.calendar.year;
        // Fire spreads less in rain (environment `+0x10`).
        town.construction.precipitation = self.environment.as_ref().is_some_and(|e| e.state == 0);
        town.apply_commands(&grid, &mut self.rng);
    }
}
