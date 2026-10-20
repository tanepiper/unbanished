//! Deterministic lockstep: several players run the same game, each on their own computer, and
//! only their commands travel. See docs/MULTIPLAYER-PLAN.md, step 3.
//!
//! The game counts updates since it loaded. A command a player gives while `c` updates have
//! begun is carried out in slot `c + delay` on every computer: after update `c + delay` and before
//! the next. When a player begins an update it seals its commands for the slot `delay` ahead into
//! a [`Turn`] and sends it. A computer carries a slot out, and begins the next update, only once it
//! holds every player's turn for that slot; the commands run by player, then in the order given.
//! The first `delay` slots are empty for everyone.
//!
//! Deviation: not in the original, which has no multiplayer. A single-player game has no
//! lockstep and carries each command out at once ([`super::commands`]).
// Until the network (step 5) joins browsers, only the command-line test drives it.
#![cfg_attr(target_arch = "wasm32", allow(dead_code))]

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::commands::PlayerCommand;
use crate::loops::GameLoop;

/// One player's commands for one slot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Turn {
    pub player: u8,
    pub slot: u64,
    pub commands: Vec<PlayerCommand>,
}

/// What the players' computers send each other.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    Turn(Turn),
    /// A player's digest of the game when `update` updates had begun and its slot had run, for
    /// the host to compare.
    /// `epoch` counts the resyncs the player has gone through: a digest from before the last
    /// resync is out of date.
    Digest {
        player: u8,
        epoch: u32,
        update: u64,
        crc: u32,
    },
    /// The host's game, saved when `update` updates had begun and its slot had run: everybody
    /// loads it and goes on from there (a resync).
    Save {
        update: u64,
        bytes: Vec<u8>,
    },
    /// The host starts the game: its save, the players, the input delay and how often the
    /// host compares digests.
    Start {
        players: u8,
        delay: u64,
        check: u64,
        bytes: Vec<u8>,
    },
}

/// Turns already carried out are kept this many slots, for a computer that was ahead and goes
/// back to a resync's save.
const HISTORY: u64 = 4096;

/// One computer's side of the lockstep.
#[derive(Debug, Clone)]
pub struct Lockstep {
    /// This computer's player.
    pub player: u8,
    /// The players, numbered from 0.
    pub players: u8,
    /// Slots between giving a command and carrying it out.
    pub delay: u64,
    /// This player's commands not yet sealed, by slot.
    local: BTreeMap<u64, Vec<PlayerCommand>>,
    /// The turns held, by slot and player.
    turns: BTreeMap<u64, BTreeMap<u8, Vec<PlayerCommand>>>,
    /// The updates count the local turns have been sealed for, up to.
    sealed: Option<u64>,
    /// The turns carried out, by slot (the last [`HISTORY`] slots).
    history: BTreeMap<u64, BTreeMap<u8, Vec<PlayerCommand>>>,
    /// The host's digests from each player, by update.
    checks: BTreeMap<u64, BTreeMap<u8, u32>>,
    /// The resyncs this computer has gone through.
    pub epoch: u32,
}

impl Lockstep {
    pub fn new(player: u8, players: u8, delay: u64) -> Lockstep {
        Lockstep {
            player,
            players,
            delay,
            local: BTreeMap::new(),
            turns: BTreeMap::new(),
            sealed: None,
            history: BTreeMap::new(),
            checks: BTreeMap::new(),
            epoch: 0,
        }
    }

    /// Seals the local turns of every updates count after the last sealed, up to `updates`
    /// (one, normally; more after a resync jumped ahead), to send to the others.
    pub fn seal_through(&mut self, updates: u64) -> Vec<Turn> {
        let from = self.sealed.map_or(0, |s| s + 1);
        let turns = (from..=updates).map(|u| self.seal(u)).collect();
        self.sealed = Some(self.sealed.map_or(updates, |s| s.max(updates)));
        turns
    }

    /// Back to a resync's save, made when `update` updates had begun and its slot had run: the
    /// turns of the slots after it are held again (a computer that was ahead carries them out
    /// once more), the ones up to it are in the save.
    pub fn rewind(&mut self, update: u64) {
        let later: Vec<u64> = self.history.range(update + 1..).map(|(&s, _)| s).collect();
        for slot in later {
            let turns = self.history.remove(&slot).unwrap();
            let held = self.turns.entry(slot).or_default();
            for (player, commands) in turns {
                held.entry(player).or_insert(commands);
            }
        }
        self.turns.retain(|&s, _| s > update);
    }

    /// The host gets a player's digest. When every player's digest of `update` is here, says
    /// whether they all agree.
    pub fn check(&mut self, player: u8, update: u64, crc: u32) -> Option<bool> {
        self.check_in(player, self.epoch, update, crc)
    }

    /// [`Lockstep::check`] for a digest made in `epoch`; one from before the host's last resync
    /// is ignored.
    pub fn check_in(&mut self, player: u8, epoch: u32, update: u64, crc: u32) -> Option<bool> {
        if epoch != self.epoch {
            return None;
        }
        let held = self.checks.entry(update).or_default();
        held.insert(player, crc);
        if held.len() < self.players as usize {
            return None;
        }
        let held = self.checks.remove(&update).unwrap();
        let first = held.values().next().copied();
        Some(held.values().all(|&c| Some(c) == first))
    }

    /// The local player gives a command while `updates` updates have begun.
    pub fn give(&mut self, updates: u64, command: PlayerCommand) {
        self.local
            .entry(updates + self.delay)
            .or_default()
            .push(command);
    }

    /// Seals the local player's turn for the slot `delay` after `updates`, before update
    /// `updates + 1` begins; it is kept and returned to be sent to the others.
    pub fn seal(&mut self, updates: u64) -> Turn {
        let slot = updates + self.delay;
        let commands = self.local.remove(&slot).unwrap_or_default();
        self.receive(Turn {
            player: self.player,
            slot,
            commands: commands.clone(),
        });
        Turn {
            player: self.player,
            slot,
            commands,
        }
    }

    /// Another player's turn arrives (in any order).
    pub fn receive(&mut self, turn: Turn) {
        self.turns
            .entry(turn.slot)
            .or_default()
            .insert(turn.player, turn.commands);
    }

    /// Every player's turn for `slot` is here (the first `delay` slots are empty).
    pub fn ready(&self, slot: u64) -> bool {
        slot < self.delay
            || self
                .turns
                .get(&slot)
                .is_some_and(|t| t.len() == self.players as usize)
    }

    /// The commands of `slot`, by player, then in the order given; the slot moves to the
    /// history.
    pub fn take(&mut self, slot: u64) -> Vec<(u8, PlayerCommand)> {
        let turns = self.turns.remove(&slot).unwrap_or_default();
        let out = turns
            .iter()
            .flat_map(|(&player, commands)| commands.iter().map(move |c| (player, c.clone())))
            .collect();
        self.history.insert(slot, turns);
        if slot >= HISTORY {
            self.history = self.history.split_off(&(slot - HISTORY));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn speed(s: u8) -> PlayerCommand {
        PlayerCommand::SetSpeed(s)
    }

    #[test]
    fn commands_run_after_the_delay_in_player_order() {
        let mut a = Lockstep::new(0, 2, 3);
        let mut b = Lockstep::new(1, 2, 3);
        // The first three slots need nobody.
        assert!(a.ready(0) && a.ready(2) && !a.ready(3));
        b.give(0, speed(2));
        a.give(0, speed(1));
        a.give(0, speed(3));
        let (ta, tb) = (a.seal(0), b.seal(0));
        assert_eq!((ta.slot, tb.slot), (3, 3));
        assert!(!a.ready(3), "b's turn has not arrived");
        a.receive(tb.clone());
        b.receive(ta.clone());
        assert!(a.ready(3) && b.ready(3));
        let order = |l: &mut Lockstep| -> Vec<(u8, String)> {
            l.take(3)
                .into_iter()
                .map(|(p, c)| (p, format!("{c:?}")))
                .collect()
        };
        let (oa, ob) = (order(&mut a), order(&mut b));
        assert_eq!(oa, ob);
        assert_eq!(
            oa,
            vec![
                (0, "SetSpeed(1)".to_string()),
                (0, "SetSpeed(3)".to_string()),
                (1, "SetSpeed(2)".to_string())
            ]
        );
        // An empty turn still counts.
        let (ta, tb) = (a.seal(1), b.seal(1));
        assert!(ta.commands.is_empty());
        a.receive(tb);
        b.receive(ta);
        assert!(a.ready(4) && b.ready(4));
    }

    #[test]
    fn a_resync_holds_the_later_turns_again_and_seals_what_was_skipped() {
        let mut a = Lockstep::new(0, 2, 2);
        for u in 0..6 {
            a.seal_through(u);
            a.receive(Turn {
                player: 1,
                slot: u + 2,
                commands: vec![speed(u as u8)],
            });
            assert!(a.ready(u));
            a.take(u);
        }
        // Back to the save made after slot 3: slots 4 and 5 run again.
        a.rewind(3);
        assert!(a.ready(4) && a.ready(5) && !a.ready(3));
        assert_eq!(a.take(4).len(), 1);
        // A computer that was behind seals every skipped updates count at once.
        let mut b = Lockstep::new(1, 2, 2);
        b.seal_through(1);
        let turns = b.seal_through(5);
        assert_eq!(
            turns.iter().map(|t| t.slot).collect::<Vec<_>>(),
            vec![4, 5, 6, 7]
        );
        assert!(b.seal_through(5).is_empty());
    }

    #[test]
    fn the_host_compares_every_players_digest() {
        let mut host = Lockstep::new(0, 3, 6);
        assert_eq!(host.check(0, 600, 7), None);
        assert_eq!(host.check(2, 600, 7), None);
        assert_eq!(host.check(1, 600, 7), Some(true));
        host.check(0, 1200, 7);
        host.check(1, 1200, 8);
        assert_eq!(host.check(2, 1200, 7), Some(false));
    }

    #[test]
    fn turns_can_arrive_out_of_order() {
        let mut a = Lockstep::new(0, 3, 2);
        for slot in [5, 3, 4, 2] {
            for player in [2, 1] {
                a.receive(Turn {
                    player,
                    slot,
                    commands: vec![speed(player)],
                });
            }
        }
        for updates in 0..4 {
            assert!(!a.ready(updates + 2) || updates + 2 < 2);
            a.seal(updates);
            assert!(a.ready(updates + 2));
        }
        let t = a.take(2);
        assert_eq!(t.len(), 2);
        assert_eq!((t[0].0, t[1].0), (1, 2));
    }
}

impl super::GameplayLoop {
    /// Plays this game in lockstep as `player` of `players`, with an input delay of `delay`
    /// updates.
    pub fn join_lockstep(&mut self, player: u8, players: u8, delay: u64) {
        self.lockstep = Some(Lockstep::new(player, players, delay));
    }

    /// The local player gives a command: in lockstep it waits for its slot.
    pub fn give(&mut self, archive: &banished_formats::pkg::Archive, command: PlayerCommand) {
        match &mut self.lockstep {
            Some(l) => l.give(self.updates, command),
            None => {
                self.execute(archive, command);
            }
        }
    }

    /// The messages since the last call, to send to the other players.
    pub fn take_outbox(&mut self) -> Vec<Message> {
        std::mem::take(&mut self.outbox)
    }

    /// Another player's turn arrives.
    pub fn receive_turn(&mut self, turn: Turn) {
        if let Some(l) = &mut self.lockstep {
            l.receive(turn);
        }
    }

    /// Before an update: seals the local turn `delay` slots ahead (once), and carries out the
    /// slot of the updates begun so far if every player's turn is here. False while waiting.
    pub fn lockstep_slot(&mut self, ctx: &mut crate::loops::Ctx) -> bool {
        let slot = self.updates;
        if self.slot_done == Some(slot) {
            return true;
        }
        let Some(l) = &mut self.lockstep else {
            return true;
        };
        let sealed = l.seal_through(slot);
        self.outbox.extend(sealed.into_iter().map(Message::Turn));
        if !l.ready(slot) {
            return false;
        }
        for (player, command) in l.take(slot) {
            self.execute_from(ctx, player, command);
        }
        self.slot_done = Some(slot);
        true
    }

    /// Carries out a player's command. A tool it starts is the local player's current tool, or
    /// kept for the other player (their Path tool; drag tools are kept by template for all).
    fn execute_from(&mut self, ctx: &mut crate::loops::Ctx, player: u8, command: PlayerCommand) {
        use super::commands::{Outcome, Tool};
        let local = self.lockstep.as_ref().is_none_or(|l| l.player == player);
        if local {
            if let Outcome::Tool(Some(tool)) = self.execute(ctx.archive, command) {
                match tool {
                    Tool::Placement(t) => self.placement = Some(t),
                    Tool::Removal(t) => self.removal = Some(t),
                    Tool::Path(t) => self.path_tool = Some(t),
                }
            }
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
        let saved = crate::saves::saved::SavedGame::parse(bytes).map_err(|e| e.to_string())?;
        let mut g = super::GameplayLoop::from_save(saved);
        g.load(ctx);
        g.startup(ctx);
        g.autosave = None;
        g.replaying = self.replaying;
        g.menu_paused = self.menu_paused;
        g.speed = self.speed;
        g.paused = self.paused;
        g.drag_tools = std::mem::take(&mut self.drag_tools);
        g.remote_paths = std::mem::take(&mut self.remote_paths);
        g.path_tool = self.path_tool.take();
        g.outbox = std::mem::take(&mut self.outbox);
        g.updates = update;
        g.slot_done = Some(update);
        if let Some(mut l) = self.lockstep.take() {
            l.rewind(update);
            l.epoch += 1;
            l.checks.clear();
            let sealed = l.seal_through(update);
            g.outbox.extend(sealed.into_iter().map(Message::Turn));
            g.lockstep = Some(l);
        }
        Ok(g)
    }
}
