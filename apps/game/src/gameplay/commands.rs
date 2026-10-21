//! Player commands in the game app: [`GameplayLoop::execute`] sends a command to the lockstep
//! slot or the recording, handles the speed, the pause and the town's new name, and passes the
//! rest to the simulation (`unbanished_sim::command`, which describes the commands).

use unbanished_formats::pkg::Archive;

pub use unbanished_sim::command::{Outcome, PlayerCommand, Tool};
pub use unbanished_sim::tools::drag::Drag;

use super::{GameplayLoop, SPEED_LEVELS};

impl GameplayLoop {
    /// The town names the Map window draws from (see `Simulation::load_town_names`).
    pub(super) fn load_town_names(
        &mut self,
        archive: &Archive,
    ) -> Option<&unbanished_formats::names::NameDatabase> {
        self.sim.load_town_names(archive)
    }

    /// Carries out a player's command now, and writes it to the recording if there is one.
    pub(super) fn execute(&mut self, archive: &Archive, mut command: PlayerCommand) -> Outcome {
        // In lockstep the player's commands wait for their slot, when every computer carries
        // them out in the same order.
        match self.route(command) {
            Some(now) => command = now,
            None => return Outcome::Queued,
        }
        self.record_command(&command);
        match command {
            PlayerCommand::RenameTown { name } => {
                self.params.town_name = name;
                Outcome::None
            }
            PlayerCommand::SetSpeed(speed) => {
                self.speed = speed.min((SPEED_LEVELS.len() - 1) as u8);
                Outcome::None
            }
            PlayerCommand::SetPaused(paused) => {
                self.paused = paused;
                Outcome::None
            }
            command => self.sim.execute(archive, command, self.path_tool.as_mut()),
        }
    }
}
