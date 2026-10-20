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

use crate::tools::removal_tool::RemovalTool;
use crate::town::StartingTown;

mod step;

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
    /// The stock manager's warning state.
    pub stock_warnings: StockWarnings,
    // StockManager +0x150
    /// The Town Info graphs' history, sampled every 75 s.
    pub statistics: Statistics,
    /// Updates begun since the game loaded (recordings and replays count by it).
    pub updates: u64,
    /// The drag tools commands have started, by template, for `PlayerCommand::Drag`.
    pub drag_tools: BTreeMap<String, RemovalTool>,
    /// The town names the Map window draws from, read the first time they are wanted.
    pub town_names: Option<NameDatabase>,
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
            stock_warnings: Default::default(),
            statistics: Default::default(),
            updates: 0,
            drag_tools: BTreeMap::new(),
            town_names: None,
        }
    }

    /// The month: `floor(elapsed / 3600 × 12)`.
    pub fn month(&self) -> usize {
        ((self.calendar.seconds() / YEAR_SECONDS * MONTHS_PER_YEAR as f32) as usize)
            .min(MONTHS_PER_YEAR - 1)
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
