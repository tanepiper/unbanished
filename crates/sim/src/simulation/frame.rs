//! One frame of the simulation with nobody watching: the speed's iterations, then what the
//! game runs once after them. The game app runs the same steps in the same order, with the
//! drawing, the interface and a scenario's goal between them (`gameplay/frame.rs`).

use unbanished_formats::pkg::Archive;
use unbanished_formats::savegame::GameState;
use unbanished_game::scheduler::ticks;

use super::{Simulation, StepEvents};
use crate::saved::SavedGame;
use crate::strings::Strings;
use crate::town::world_params::WorldParams;

/// What a frame did that the player sees.
#[derive(Debug, Default)]
pub struct FrameEvents {
    /// The iterations' events together, in order.
    pub step: StepEvents,
    /// AutoSlow asks for speed 1: a fire, a blight, a tornado or a shortage.
    pub slow: bool,
}

impl Simulation {
    /// One frame: `iterations` iterations of `dt`, the achievements with the time they took,
    /// the phase after them, then the events posted. With `auto_slow` the events ask to slow
    /// the game down.
    ///
    /// Original: `FUN_14009b850`.
    pub fn frame(
        &mut self,
        archive: &Archive,
        strings: &mut dyn Strings,
        params: &WorldParams,
        iterations: u32,
        dt: f32,
        auto_slow: bool,
    ) -> FrameEvents {
        let mut out = FrameEvents::default();
        for _ in 0..iterations {
            let _p = crate::timing::scope("sim.iterate");
            let step = self.iterate(archive, dt);
            out.step.models_changed |= step.models_changed;
            out.step.reshaped |= step.reshaped;
            out.step.sounds.extend(step.sounds);
        }
        self.update_achievements(archive, strings, params, ticks(dt * iterations as f32));
        out.step.models_changed |= self.smooth();
        self.log_life_events(strings, auto_slow);
        self.log_site_events(archive, strings, auto_slow);
        out.slow = std::mem::take(&mut self.events.slow_request) && auto_slow;
        out
    }

    /// The state a loaded game saves now, without what only the game app knows (the camera,
    /// the windows, a scenario's goal).
    pub fn saved_state(
        &self,
        archive: &Archive,
        strings: &mut dyn Strings,
        saved: &SavedGame,
        town_name: &str,
    ) -> GameState {
        let mut g = saved.state.clone();
        self.write_loaded_achievements(&mut g);
        g.town_name = town_name.to_string();
        if let Some(town) = &self.town {
            town.export_into(archive, &mut g);
        }
        self.write_state(strings, &mut g);
        g
    }
}
