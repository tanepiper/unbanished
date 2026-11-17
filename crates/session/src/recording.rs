//! A recording: a game's player commands and digests, written as it is played, to replay it
//! exactly from its save.
//!
//! Deviation: not in the original.

use serde::{Deserialize, Serialize};
use unbanished_sim::command::PlayerCommand;

use crate::digest::Digest;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Entry {
    /// The engine that recorded it.
    Start {
        engine: String,
    },
    Command {
        update: u64,
        /// Whose command it was in a multiplayer game; `None` in a single-player one, and in
        /// a recording made before the number was written.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        player: Option<u8>,
        command: PlayerCommand,
    },
    /// From the start of update `update + 1`, the pause menu holds (or no longer holds) the game.
    MenuPause {
        update: u64,
        paused: bool,
    },
    Digest(Digest),
}

/// A recording being written.
pub struct Recorder {
    out: Box<dyn std::io::Write>,
    /// A digest every this many updates (0: none).
    pub every: u64,
}

impl Recorder {
    /// A recording written to `out` by the engine `engine` (its name and version), with a
    /// digest every `every` updates.
    pub fn new(mut out: Box<dyn std::io::Write>, engine: &str, every: u64) -> Recorder {
        let start = Entry::Start {
            engine: engine.to_string(),
        };
        write(&mut out, &start);
        Recorder { out, every }
    }

    pub fn command(&mut self, update: u64, player: Option<u8>, command: &PlayerCommand) {
        let entry = Entry::Command {
            update,
            player,
            command: command.clone(),
        };
        write(&mut self.out, &entry);
    }

    pub fn menu_pause(&mut self, update: u64, paused: bool) {
        write(&mut self.out, &Entry::MenuPause { update, paused });
    }

    pub fn digest(&mut self, digest: Digest) {
        write(&mut self.out, &Entry::Digest(digest));
    }
}

fn write(out: &mut Box<dyn std::io::Write>, entry: &Entry) {
    let line = serde_json::to_string(entry).expect("an entry encodes");
    if let Err(e) = writeln!(out, "{line}").and_then(|_| out.flush()) {
        log::error!("recording: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_multiplayer_command_keeps_its_player() {
        let entry = Entry::Command {
            update: 3,
            player: Some(2),
            command: PlayerCommand::SetPaused(true),
        };
        let line = serde_json::to_string(&entry).unwrap();
        assert!(line.contains("\"player\":2"), "{line}");
        let back: Entry = serde_json::from_str(&line).unwrap();
        assert!(matches!(back, Entry::Command { player: Some(2), .. }));
    }

    #[test]
    fn a_single_player_command_has_no_player() {
        let entry = Entry::Command {
            update: 3,
            player: None,
            command: PlayerCommand::SetPaused(true),
        };
        let line = serde_json::to_string(&entry).unwrap();
        assert!(!line.contains("player"), "{line}");
        let back: Entry = serde_json::from_str(&line).unwrap();
        assert!(matches!(back, Entry::Command { player: None, .. }));
    }

    #[test]
    fn an_old_recording_reads_as_single_player() {
        let line = r#"{"command":{"update":3,"command":{"SetPaused":true}}}"#;
        let back: Entry = serde_json::from_str(line).unwrap();
        assert!(matches!(back, Entry::Command { player: None, .. }));
    }
}
