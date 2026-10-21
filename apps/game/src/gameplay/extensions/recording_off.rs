//! A build without the `replay` feature: nothing is recorded or replayed.

use super::super::*;

/// No recording.
#[derive(Default)]
pub struct Recording;

impl GameplayLoop {
    /// Nothing to start.
    pub(in crate::gameplay) fn start_recording(&mut self) {}

    /// No digests to record.
    pub(in crate::gameplay) fn record_digest(&mut self, _ctx: &mut Ctx, _held: bool) {}

    /// Not a replay.
    pub(in crate::gameplay) fn replaying(&self) -> bool {
        false
    }

    /// Nothing to record.
    pub(in crate::gameplay) fn record_command(&mut self, _command: &PlayerCommand) {}

    /// Nothing to record.
    pub(in crate::gameplay) fn record_menu_pause(&mut self, _update: u64, _paused: bool) {}
}
