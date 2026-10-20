//! The smoothing pass after each iteration (it draws on the game's `Random`, so it is simulation).

use super::*;

impl StartingTown {
    /// FUN_1400c98a0's post-iteration phase; also runs when the toolbar pauses simulation.
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
