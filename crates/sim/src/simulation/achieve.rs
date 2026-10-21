//! The achievements, checked after each frame's iterations.

use unbanished_formats::pkg::Archive;

use super::Simulation;
use crate::strings::Strings;
use crate::town::world_params::WorldParams;

impl Simulation {
    /// Reads the achievement list; `awarded` says whether the achievement with this title was
    /// awarded before. A save restores its progress.
    pub fn load_achievements(
        &mut self,
        archive: &Archive,
        saved: Option<&crate::saved::SavedGame>,
        awarded: impl Fn(&str) -> bool,
    ) {
        self.achievements = crate::achievements::load(archive);
        self.achievement_manager =
            crate::achievements::AchievementManager::new(&self.achievements, awarded);
        if let Some(saved) = saved {
            self.achievement_manager.restore(&self.achievements, saved);
        }
    }

    /// `AchievementManager::Update(dt × iterations)` after the frame's iterations, for the
    /// game started with `params`. One awarded is logged (event `0x19`, `@2` its title) and
    /// returned, for the game app to keep. Nothing is checked, nor any time gathered, while
    /// user pack files are enabled.
    ///
    /// Original: `FUN_14009be30`.
    pub fn update_achievements(
        &mut self,
        archive: &Archive,
        strings: &mut dyn Strings,
        params: &WorldParams,
        dt: i64,
    ) -> Option<usize> {
        // FileSystem +0x38
        if 0 < archive.user_packs {
            return None;
        }
        let town = self.town.as_ref()?;
        // The TerrainGenerator's resource is named for the terrain.
        let terrain = format!(r"Game\Terrain\{}.rsc", params.terrain);
        let x = crate::achievements::GoalContext {
            town,
            statistics: &self.statistics,
            year: self.calendar.year,
            year_seconds: self.calendar.seconds(),
            terrain: &terrain,
            climate: &params.climate_ref,
            size: params.size as i32,
        };
        let won = self
            .achievement_manager
            .update(&self.achievements, dt, &x, Some(archive))?;
        let a = &self.achievements[won].desc;
        let title = strings.string(a.strings.as_deref(), &a.title);
        let (month, year) = (self.month(), self.calendar.year as i32);
        self.events.add(
            0x19,
            4,
            -1,
            month,
            year,
            [title, String::new(), String::new()],
            false,
        );
        Some(won)
    }
}
