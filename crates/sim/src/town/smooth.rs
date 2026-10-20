//! The smoothing pass after each iteration (it draws on the game's `Random`, so it is simulation).

use super::*;

impl StartingTown {
    /// The phase after each iteration; also runs when the toolbar pauses the simulation.
    ///
    /// Original: `FUN_1400c98a0`.
    pub fn smooth(&mut self, terrain: &HeightGrid, rng: &mut Random) -> bool {
        Seeder {
            templates: &self.templates,
            terrain,
            cells: &mut self.cells,
            rng,
            placed: &mut self.placed,
            removed: &mut self.removed,
            animation_events: &mut self.animation_events,
        }
        .smooth()
    }
}
