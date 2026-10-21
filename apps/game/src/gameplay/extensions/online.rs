//! A multiplayer game's state and the game loop's door to it (the `multiplayer` feature).

use super::super::*;

/// A multiplayer game's state.
#[derive(Default)]
pub struct Online {
    /// The session ([`super::super::lockstep`]): the turns, the turns sealed to send and the slot
    /// carried out last. A single-player game has none and carries commands out at once.
    pub lockstep: Option<unbanished_session::session::LockstepSession>,
    /// The network side ([`super::super::network`]), and the update boundary it last handled.
    pub net: Option<super::super::network::NetState>,
    pub boundary_done: Option<u64>,
    /// Commands of a lockstep slot are being carried out (others go to the lockstep).
    pub carrying_out: bool,
    /// Replaced by a resync's save: it waits for the switch.
    pub retired: bool,
    /// The other players' Path tools (a Path tool's pick depends on its last pick).
    pub remote_paths: std::collections::BTreeMap<u8, crate::tools::path_tool::PathTool>,
}

impl GameplayLoop {
    /// At start-up: a multiplayer game's players line.
    pub(in crate::gameplay) fn start_online(&mut self, ctx: &mut Ctx) {
        if self.online.net.is_some() {
            self.open_players_line(ctx.ui);
        }
    }

    /// Before an update: in a multiplayer game, what the other players sent, the slot before
    /// this update (which waits for every player's turn), the turns to send and the host's
    /// checks. `Ok(true)` while the slot is held: the view and the windows go on, but nothing
    /// that changes the town runs, so every computer runs it the same number of times. `Err`
    /// is what the frame returns at once (a resync's reload, a test game waiting).
    pub(in crate::gameplay) fn begin_online(&mut self, ctx: &mut Ctx) -> Result<bool, Next> {
        if self.online.retired {
            return Err(Next::Stay);
        }
        // A multiplayer game: what the other players sent, then the slot before this update
        // (which waits for every player's turn), the turns to send, and the host's checks.
        let online = self.online.net.as_ref().is_some_and(|n| !n.driven);
        if online {
            if let (_, Some((bytes, update))) = self.net_poll() {
                return Err(self.switch_reload(ctx, &bytes, update));
            }
        }
        // Held: the slot waits for another player's turn. The view and the windows go on, but
        // nothing that changes the town runs, so every computer runs it the same number of times.
        let held = self.online.lockstep.is_some() && !self.lockstep_slot(ctx);
        if held && !online {
            // A driven test game ([`network::NetPeer`]) only updates when its slot is ready.
            return Err(Next::Stay);
        }
        if online {
            self.net_send();
            self.refresh_players_line(ctx.ui, held);
            if !held {
                match self.net_boundary(ctx, self.sim.updates) {
                    Ok(Some((bytes, update))) => {
                        return Err(self.switch_reload(ctx, &bytes, update))
                    }
                    Ok(None) => {}
                    Err(e) => log::error!("multiplayer: {e}"),
                }
            }
        }
        Ok(held)
    }

    /// A multiplayer game.
    pub(in crate::gameplay) fn online(&self) -> bool {
        self.online.net.is_some()
    }

    /// A game in lockstep: its commands wait for their slot.
    pub(in crate::gameplay) fn in_lockstep(&self) -> bool {
        self.online.lockstep.is_some()
    }

    /// The local player's command: `Some` to carry out now, `None` when it waits for its
    /// lockstep slot, when every computer carries it out in the same order.
    pub(in crate::gameplay) fn route(&mut self, command: PlayerCommand) -> Option<PlayerCommand> {
        use unbanished_session::session::Session;
        match &mut self.online.lockstep {
            Some(session) if !self.online.carrying_out => session.give(self.sim.updates, command),
            _ => Some(command),
        }
    }
}
