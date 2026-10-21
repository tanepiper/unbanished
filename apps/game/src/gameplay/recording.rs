//! Recording a game's player commands, and replaying them.
//!
//! A recording starts when a save is loaded (`--record FILE`). Each line of the file is one
//! JSON entry: the start, then every [`PlayerCommand`] with the update it was given in, the
//! updates the pause menu held the game, and, every few updates (`--record-every N`), a digest of
//! the game state ([`super::digest`]). `unbanished --replay SAVE FILE` loads the same save without a
//! window, carries the commands out at the same updates, and compares its digests with the
//! recorded ones. Equal digests show that the commands carry everything a player changes: the
//! same save and the same commands give the same game. See docs/MULTIPLAYER-PLAN.md, step 2.
//!
//! Deviation: not in the original; recordings are a tool of the port (replays, bug reports, a
//! step towards multiplayer).
// Recording and replaying run from the command line; the browser build only carries the types.
#![cfg_attr(target_arch = "wasm32", allow(dead_code))]

use super::commands::{Outcome, PlayerCommand, Tool};
use super::digest::Digest;
use crate::loops::GameLoop;
pub use unbanished_session::recording::{Entry, Recorder};

/// One line of a recording. `update` counts the updates since the save was loaded: a command
/// with update `n` was given after `n` updates had begun, and a digest with update `n` is the
/// state when the next update begins.
/// The recording asked for on the command line (`--record FILE`, `--record-every N`), for the
/// next game loaded from a save.
#[cfg(not(target_arch = "wasm32"))]
static REQUEST: std::sync::Mutex<Option<(std::path::PathBuf, u64)>> = std::sync::Mutex::new(None);

#[cfg(not(target_arch = "wasm32"))]
pub fn request(path: std::path::PathBuf, every: u64) {
    *REQUEST.lock().unwrap() = Some((path, every));
}

/// The recorder for a game that has just loaded, if one was asked for.
pub fn take_request() -> Option<Recorder> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let (path, every) = REQUEST.lock().unwrap().take()?;
        match std::fs::File::create(&path) {
            Ok(file) => {
                log::info!("recording to {}", path.display());
                Some(Recorder::new(
                    Box::new(std::io::BufWriter::new(file)),
                    crate::platform::version::NAME_VERSION,
                    every,
                ))
            }
            Err(e) => {
                log::error!("recording to {}: {e}", path.display());
                None
            }
        }
    }
    #[cfg(target_arch = "wasm32")]
    None
}

/// What a replay found.
pub struct Replay {
    /// Its own digests, at the recording's digest updates (and every `every` updates).
    pub digests: Vec<Digest>,
    /// The first recorded digest it does not match: (recorded, replayed).
    pub mismatch: Option<(Digest, Digest)>,
    /// Recorded digests it compared.
    pub compared: usize,
}

/// Loads `save` into `game`'s context, carries the recording's commands out at their updates,
/// and compares the game state with the recorded digests. It runs to the last recorded update,
/// or `updates` if that is more; `every` adds digests of its own.
pub fn replay(
    game: &mut crate::Game,
    save: &[u8],
    recording: &str,
    updates: u64,
    every: u64,
) -> Result<Replay, String> {
    let mut entries = Vec::new();
    for (i, line) in recording.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let entry: Entry =
            serde_json::from_str(line).map_err(|e| format!("line {}: {e}", i + 1))?;
        entries.push(entry);
    }
    let last = entries
        .iter()
        .filter_map(|e| match e {
            Entry::Command { update, .. } | Entry::MenuPause { update, .. } => Some(*update),
            Entry::Digest(d) => Some(d.update as u64),
            Entry::Start { .. } => None,
        })
        .max()
        .unwrap_or(0)
        .max(updates);
    let saved = crate::saves::saved::SavedGame::parse(save).map_err(|e| e.to_string())?;
    let mut gameplay = super::GameplayLoop::from_save(saved);
    let step = unbanished_game::scheduler::seconds(crate::FRAME_STEP);
    game.with_ctx(|_, ctx| {
        gameplay.load(ctx);
        gameplay.startup(ctx);
        gameplay.autosave = None;
        gameplay.recording.replaying = true;
        let mut out = Replay {
            digests: Vec::new(),
            mismatch: None,
            compared: 0,
        };
        let mut next = entries.iter().peekable();
        let mut update = 0;
        loop {
            // The commands given after this many updates, in their order, and the digests of
            // the state the next update begins with.
            while let Some(entry) = next.next_if(|e| entry_update(e).is_none_or(|u| u <= update)) {
                match entry {
                    Entry::Command { command, .. } => {
                        gameplay.replay_command(ctx, command.clone());
                    }
                    Entry::MenuPause { paused, .. } => gameplay.menu_paused = *paused,
                    Entry::Digest(recorded) => {
                        let ours = gameplay.digest(ctx, update as u32)?;
                        out.compared += 1;
                        if ours.crc != recorded.crc && out.mismatch.is_none() {
                            out.mismatch = Some((*recorded, ours));
                        }
                        out.digests.push(ours);
                    }
                    Entry::Start { .. } => {}
                }
            }
            if every > 0
                && update % every == 0
                && out.digests.last().is_none_or(|d| d.update as u64 != update)
            {
                out.digests.push(gameplay.digest(ctx, update as u32)?);
            }
            if update >= last {
                break;
            }
            gameplay.update(ctx, step);
            update += 1;
        }
        Ok(out)
    })
}

fn entry_update(entry: &Entry) -> Option<u64> {
    match entry {
        Entry::Command { update, .. } | Entry::MenuPause { update, .. } => Some(*update),
        Entry::Digest(d) => Some(d.update as u64),
        Entry::Start { .. } => None,
    }
}

impl super::GameplayLoop {
    /// A recorded command, carried out as the interface would: a tool it starts becomes the
    /// current one.
    fn replay_command(&mut self, ctx: &mut crate::loops::Ctx, command: PlayerCommand) {
        if let Outcome::Tool(Some(tool)) = self.execute(ctx.archive, command) {
            match tool {
                Tool::Placement(t) => self.placement = Some(t),
                Tool::Removal(t) => self.removal = Some(t),
                Tool::Path(t) => self.path_tool = Some(t),
            }
        }
    }
}
