//! A build without the `multiplayer` feature: every game is a single player's.

use super::super::*;

/// No multiplayer game.
#[derive(Default)]
pub struct Online;

impl GameplayLoop {
    /// Nothing to start.
    pub(in crate::gameplay) fn start_online(&mut self, _ctx: &mut Ctx) {}

    /// Every update runs.
    pub(in crate::gameplay) fn begin_online(&mut self, _ctx: &mut Ctx) -> Result<bool, Next> {
        Ok(false)
    }

    /// Not a multiplayer game.
    pub(in crate::gameplay) fn online(&self) -> bool {
        false
    }

    /// Not in lockstep.
    pub(in crate::gameplay) fn in_lockstep(&self) -> bool {
        false
    }

    /// Every command is carried out at once.
    pub(in crate::gameplay) fn route(&mut self, command: PlayerCommand) -> Option<PlayerCommand> {
        Some(command)
    }
}
