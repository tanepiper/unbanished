//! Loading a game into the simulation: the town's flags and the traders' river, the
//! environment bound to the climate, and a whole saved game.

use unbanished_formats::game::climate_generator;
use unbanished_formats::pkg::Archive;
use unbanished_formats::Result;
use unbanished_game::nature::environment::{Climate, Environment};

use super::Simulation;
use crate::saved::SavedGame;
use crate::town::map::Map;
use crate::town::world_params::WorldParams;
use crate::town::StartingTown;

impl Simulation {
    /// The town's game flags (a save's own, else from the Disasters setting) and the trader
    /// boats' way, the map's first river.
    pub fn settle_town(
        &mut self,
        saved: Option<&SavedGame>,
        params: &WorldParams,
        river_paths: &[Vec<[f32; 2]>],
    ) {
        let Some(town) = &mut self.town else { return };
        // Game +0xd0
        town.construction.game_flags = match saved {
            Some(s) => s.state.flags,
            None if params.disasters => 0,
            None => 0xf0,
        };
        // MapComponent +0x1e0
        town.construction.river = river_paths.first().cloned().unwrap_or_default();
    }

    /// Binds the environment to the selected climate, with the weather list's fade times; a
    /// save restores its own state.
    ///
    /// Original: `FUN_14009bfd0`, `FUN_140099300`.
    pub fn bind_environment(
        &mut self,
        archive: &Archive,
        params: &WorldParams,
        weather: &[(Option<String>, Option<String>)],
        saved: Option<&SavedGame>,
    ) -> Result<()> {
        let reference = params.climate_ref.replace('\\', "/");
        let c = climate_generator(&archive.read_ref(&reference)?)?;
        let mut env = Environment::new(Climate::from(&c));
        // Weather +0x5c
        // The weather list's descriptions: their fade times.
        for (slot, (weather, _)) in weather.iter().enumerate().take(2) {
            env.fade_times[slot] = weather
                .as_deref()
                .and_then(|r| archive.read_ref(&r.replace('\\', "/")).ok())
                .and_then(|d| unbanished_formats::weather::weather(&d).ok())
                .map(|w| w.fade_time);
        }
        // Game +0xd0 bit 7
        // Disasters off stop tornadoes.
        env.game_flags = if params.disasters { 0 } else { 0xf0 };
        if let Some(saved) = saved {
            env.game_flags = saved.state.flags;
            saved.restore_environment(&mut env);
        }
        self.environment = Some(env);
        Ok(())
    }

    /// A saved game, loaded as the game app loads it, with no drawing data: the map, the town
    /// and the environment, then the state the save keeps beside them.
    pub fn from_save(archive: &Archive, saved: &SavedGame) -> Result<Simulation> {
        let params = saved.params();
        let mut sim = Simulation::new(params.seed);
        sim.restore(saved);
        let map = Map::from_save(archive, saved)?;
        sim.environment_template = map.environment.template.clone();
        sim.town = StartingTown::from_save(archive, saved);
        sim.settle_town(Some(saved), &params, &map.river_paths);
        sim.heights = Some(map.heights);
        sim.bind_environment(archive, &params, &map.environment.weather, Some(saved))?;
        sim.environment_months = map.environment.months;
        Ok(sim)
    }
}
