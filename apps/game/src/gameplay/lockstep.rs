//! The game loop's side of lockstep (`unbanished_session::lockstep`): joining it, giving it
//! the player's commands, carrying out each slot's commands, and reloading for a resync.
//!
//! Deviation: not in the original, which has no multiplayer.
#![cfg_attr(target_arch = "wasm32", allow(dead_code))]

pub use unbanished_session::lockstep::{Message, Turn};
use unbanished_session::session::{LockstepSession, Session};

use super::commands::PlayerCommand;
use crate::loops::GameLoop;

impl super::GameplayLoop {
    /// Plays this game in lockstep as `player` of `players`, with an input delay of `delay`
    /// updates.
    pub fn join_lockstep(&mut self, player: u8, players: u8, delay: u64) {
        self.lockstep = Some(LockstepSession::new(player, players, delay));
    }

    /// The local player gives a command: in lockstep it waits for its slot.
    pub fn give(&mut self, archive: &unbanished_formats::pkg::Archive, command: PlayerCommand) {
        match &mut self.lockstep {
            Some(l) => {
                l.give(self.sim.updates, command);
            }
            None => {
                self.execute(archive, command);
            }
        }
    }

    /// The messages since the last call, to send to the other players.
    pub fn take_outbox(&mut self) -> Vec<Message> {
        self.lockstep
            .as_mut()
            .map_or_else(Vec::new, |l| l.take_outbox())
    }

    /// Another player's turn arrives.
    pub fn receive_turn(&mut self, turn: Turn) {
        if let Some(l) = &mut self.lockstep {
            l.lockstep.receive(turn);
        }
    }

    /// Before an update: seals the local turn `delay` slots ahead (once), and carries out the
    /// slot of the updates begun so far if every player's turn is here. False while waiting.
    pub fn lockstep_slot(&mut self, ctx: &mut crate::loops::Ctx) -> bool {
        let Some(l) = &mut self.lockstep else {
            return true;
        };
        let Some(commands) = l.slot(self.sim.updates) else {
            return false;
        };
        self.carrying_out = true;
        for (player, command) in commands {
            self.execute_from(ctx, player, command);
        }
        self.carrying_out = false;
        true
    }

    /// Carries out a player's command. A tool it starts is the local player's current tool, or
    /// kept for the other player (their Path tool; drag tools are kept by template for all).
    fn execute_from(&mut self, ctx: &mut crate::loops::Ctx, player: u8, command: PlayerCommand) {
        use super::commands::{Outcome, Tool};
        let local = self.lockstep.as_ref().is_none_or(|l| l.player() == player);
        if local {
            let template = match &command {
                PlayerCommand::StartTool { template } => Some(template.clone()),
                _ => None,
            };
            let outcome = self.execute(ctx.archive, command);
            self.local_outcome(ctx, template.as_deref(), outcome);
            return;
        }
        let mine = std::mem::replace(&mut self.path_tool, self.remote_paths.remove(&player));
        if let Outcome::Tool(Some(Tool::Path(t))) = self.execute(ctx.archive, command) {
            self.path_tool = Some(t);
        }
        if let Some(t) = std::mem::replace(&mut self.path_tool, mine) {
            self.remote_paths.insert(player, t);
        }
    }
}

impl super::GameplayLoop {
    /// What the local player's command did, now that its slot ran: the tool it started, the
    /// sound of a drag, a random town name for the town window.
    fn local_outcome(
        &mut self,
        ctx: &mut crate::loops::Ctx,
        template: Option<&str>,
        outcome: super::commands::Outcome,
    ) {
        use super::commands::Outcome;
        match outcome {
            Outcome::Tool(_) => self.tool_started(ctx, template.unwrap_or(""), outcome),
            Outcome::Sound(s) => self.play_drag_sound(ctx, s),
            Outcome::Name(name) => {
                if let Some(super::MenuDialog::Map(m)) = &self.menu_dialog {
                    m.set_name(ctx.ui, &name);
                }
            }
            Outcome::None | Outcome::Placed(_) | Outcome::Queued => {}
        }
    }
}

impl super::GameplayLoop {
    /// This game replaced by a resync's save, made when `update` updates had begun and its slot
    /// had run. The lockstep, the tools, the speed and the pause go on (they are not in saves, and
    /// are the same on every computer); turns of later slots are held again, and the local turns
    /// up to `update` are sealed if they were not (for a computer that was behind).
    pub fn reloaded(
        mut self,
        ctx: &mut crate::loops::Ctx,
        bytes: &[u8],
        update: u64,
    ) -> Result<super::GameplayLoop, String> {
        let mut g = self.reloading(bytes, update)?;
        g.load(ctx);
        g.startup(ctx);
        Ok(g)
    }

    /// The real game's resync: it fades to the loading screen and on to the host's save.
    pub(super) fn switch_reload(
        &mut self,
        ctx: &mut crate::loops::Ctx,
        bytes: &[u8],
        update: u64,
    ) -> crate::loops::Next {
        match self.reloading(bytes, update) {
            Ok(g) => {
                log::info!("multiplayer: resyncing from the host's save at update {update}");
                crate::loops::Next::Switch(Box::new(crate::loops::DialogLoaderLoop::new(
                    format!("Dialog/Loading.rsc:gameLoad{}", ctx.rng.rand_below(12)),
                    0.0,
                    Box::new(g),
                )))
            }
            Err(e) => {
                log::error!("multiplayer: the host's save: {e}");
                crate::loops::Next::Stay
            }
        }
    }

    /// [`Self::reloaded`] without loading: the game to switch to (the real game shows the loading
    /// screen meanwhile). This game is left retired.
    pub fn reloading(&mut self, bytes: &[u8], update: u64) -> Result<super::GameplayLoop, String> {
        let saved = crate::saves::saved::SavedGame::parse(bytes).map_err(|e| e.to_string())?;
        let mut g = super::GameplayLoop::from_save(saved);
        g.autosave = None;
        g.replaying = self.replaying;
        g.menu_paused = self.menu_paused;
        g.speed = self.speed;
        g.paused = self.paused;
        g.sim.drag_tools = std::mem::take(&mut self.sim.drag_tools);
        g.remote_paths = std::mem::take(&mut self.remote_paths);
        g.path_tool = self.path_tool.take();
        g.net = self.net.take();
        if let Some(net) = &mut g.net {
            net.forget_line();
        }
        g.sim.updates = update;
        g.boundary_done = Some(update);
        if let Some(mut l) = self.lockstep.take() {
            l.resynced(update);
            g.lockstep = Some(l);
        }
        self.retired = true;
        Ok(g)
    }
}
