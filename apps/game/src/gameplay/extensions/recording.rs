//! A recording's state and the game loop's door to it (the `replay` feature).

use super::super::*;

/// A recording being written or replayed.
#[derive(Default)]
pub struct Recording {
    /// A replay ([`super::super::recording::replay`]): the recording, not the interface, gives the
    /// commands.
    pub replaying: bool,
    /// The recording being written (`--record`).
    pub recorder: Option<super::super::recording::Recorder>,
}

impl GameplayLoop {
    /// At start-up: a recording asked for on the command line (`--record`: a game loaded from
    /// a save is recorded from its first update).
    pub(in crate::gameplay) fn start_recording(&mut self) {
        if crate::platform::mode::vanilla() {
            return;
        }
        if self.saved.is_some() && !self.recording.replaying {
            self.recording.recorder = super::super::recording::take_request();
        }
    }

    /// A recording's digest of the state this update begins with, every few updates.
    pub(in crate::gameplay) fn record_digest(&mut self, ctx: &mut Ctx, held: bool) {
        // A recording's digest of the state this update begins with, every few updates.
        if !held
            && self
                .recording
                .recorder
                .as_ref()
                .is_some_and(|r| r.every > 0 && self.sim.updates % r.every == 0)
        {
            match self.digest(ctx, self.sim.updates as u32) {
                Ok(d) => self.recording.recorder.as_mut().unwrap().digest(d),
                Err(e) => log::warn!("recording: {e}"),
            }
        }
    }

    /// A replay: the recording gives the commands and the pauses.
    pub(in crate::gameplay) fn replaying(&self) -> bool {
        self.recording.replaying
    }

    /// Writes a command carried out to the recording.
    pub(in crate::gameplay) fn record_command(&mut self, command: &PlayerCommand) {
        if let Some(r) = &mut self.recording.recorder {
            r.command(self.sim.updates, command);
        }
    }

    /// Writes the pause menu's hold or release to the recording.
    pub(in crate::gameplay) fn record_menu_pause(&mut self, update: u64, paused: bool) {
        if let Some(r) = &mut self.recording.recorder {
            r.menu_pause(update, paused);
        }
    }
}
