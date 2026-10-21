//! When a game carries out its players' commands. In a single-player game ([`Local`]) a command
//! is carried out at once, where the player gave it. In a multiplayer game ([`LockstepSession`])
//! it waits for its slot, and every computer carries the same commands out at the same update
//! (`crate::lockstep`). The game asks its session before each update and with each command; the
//! session never touches the game itself, so a game app, a test or a server can drive one.
//!
//! Deviation: multiplayer is not in the original; a single-player game behaves as it does.

use unbanished_sim::command::PlayerCommand;

use crate::lockstep::{Lockstep, Message};

/// When a game carries out its players' commands.
pub trait Session {
    /// The local player.
    fn player(&self) -> u8;
    /// The local player gave `command` while `update` updates had begun. `Some` gives it back
    /// to carry out now; `None` keeps it for its slot.
    fn give(&mut self, update: u64, command: PlayerCommand) -> Option<PlayerCommand>;
    /// Before update `update`: the commands to carry out first, by player, or `None` while
    /// another player's turn is missing (the update must wait). A slot's commands are given
    /// once; asking again gives none.
    fn slot(&mut self, update: u64) -> Option<Vec<(u8, PlayerCommand)>>;
    /// The messages for the other players since the last call.
    fn take_outbox(&mut self) -> Vec<Message>;
}

/// A single-player game: every command is carried out at once.
#[derive(Debug, Default)]
pub struct Local;

impl Session for Local {
    fn player(&self) -> u8 {
        0
    }

    fn give(&mut self, _update: u64, command: PlayerCommand) -> Option<PlayerCommand> {
        Some(command)
    }

    fn slot(&mut self, _update: u64) -> Option<Vec<(u8, PlayerCommand)>> {
        Some(Vec::new())
    }

    fn take_outbox(&mut self) -> Vec<Message> {
        Vec::new()
    }
}

/// A multiplayer game in lockstep: commands wait for their slot.
pub struct LockstepSession {
    /// The players' turns and the host's checks.
    pub lockstep: Lockstep,
    outbox: Vec<Message>,
    /// The last slot carried out.
    slot_done: Option<u64>,
}

impl LockstepSession {
    /// A lockstep game as `player` of `players`, with an input delay of `delay` updates.
    pub fn new(player: u8, players: u8, delay: u64) -> LockstepSession {
        LockstepSession {
            lockstep: Lockstep::new(player, players, delay),
            outbox: Vec::new(),
            slot_done: None,
        }
    }

    /// This computer joins a running game from a save made when `update` updates had begun and
    /// its slot had run, after `epoch` resyncs (`Lockstep::start_at`).
    pub fn start_at(&mut self, update: u64, epoch: u32) {
        self.lockstep.start_at(update, epoch);
        self.slot_done = Some(update);
    }

    /// The game was replaced by a resync's save, made when `update` updates had begun and its
    /// slot had run. The turns of later slots are held again, the checks start over, and the
    /// local turns up to `update` are sealed if they were not (for a computer that was behind).
    pub fn resynced(&mut self, update: u64) {
        let l = &mut self.lockstep;
        l.rewind(update);
        l.epoch += 1;
        l.forget_checks();
        let sealed = l.seal_through(update);
        self.outbox.extend(sealed.into_iter().map(Message::Turn));
        self.slot_done = Some(update);
    }
}

impl Session for LockstepSession {
    fn player(&self) -> u8 {
        self.lockstep.player
    }

    fn give(&mut self, update: u64, command: PlayerCommand) -> Option<PlayerCommand> {
        self.lockstep.give(update, command);
        None
    }

    fn slot(&mut self, update: u64) -> Option<Vec<(u8, PlayerCommand)>> {
        if self.slot_done == Some(update) {
            return Some(Vec::new());
        }
        let sealed = self.lockstep.seal_through(update);
        self.outbox.extend(sealed.into_iter().map(Message::Turn));
        if !self.lockstep.ready(update) {
            return None;
        }
        let commands = self.lockstep.take(update);
        self.slot_done = Some(update);
        Some(commands)
    }

    fn take_outbox(&mut self) -> Vec<Message> {
        std::mem::take(&mut self.outbox)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn speed(s: u8) -> PlayerCommand {
        PlayerCommand::SetSpeed(s)
    }

    #[test]
    fn a_single_player_game_carries_a_command_out_at_once() {
        let mut s = Local;
        assert!(matches!(
            s.give(5, speed(1)),
            Some(PlayerCommand::SetSpeed(1))
        ));
        assert!(s.slot(5).is_some_and(|c| c.is_empty()));
        assert!(s.take_outbox().is_empty());
    }

    #[test]
    fn a_lockstep_command_waits_for_its_slot() {
        // One player, an input delay of 2 updates.
        let mut s = LockstepSession::new(0, 1, 2);
        assert!(s.give(0, speed(2)).is_none());
        for update in 0..2 {
            assert!(s.slot(update).is_some_and(|c| c.is_empty()));
        }
        let slot = s.slot(2).expect("the only player's turn is here");
        assert!(matches!(slot[..], [(0, PlayerCommand::SetSpeed(2))]));
        // A slot is given once.
        assert!(s.slot(2).is_some_and(|c| c.is_empty()));
        assert!(!s.take_outbox().is_empty(), "the sealed turns are to send");
    }

    #[test]
    fn a_lockstep_slot_waits_for_every_players_turn() {
        let mut s = LockstepSession::new(0, 2, 1);
        assert!(s.slot(0).is_some());
        assert!(s.slot(1).is_none(), "player 2's turn for slot 1 is missing");
    }

    #[test]
    fn a_player_who_joins_is_waited_for_from_its_first_slot() {
        // Two players, delay 1; player 2 joins from slot 10.
        let mut host = LockstepSession::new(0, 2, 1);
        host.lockstep.add(2, 10);
        for slot in 0..10 {
            host.lockstep.receive(crate::lockstep::Turn {
                player: 1,
                slot,
                commands: Vec::new(),
            });
            assert!(
                host.slot(slot).is_some(),
                "slot {slot} waits for nobody new"
            );
        }
        host.lockstep.receive(crate::lockstep::Turn {
            player: 1,
            slot: 10,
            commands: Vec::new(),
        });
        assert!(host.slot(10).is_none(), "slot 10 waits for the new player");
        assert_eq!(host.lockstep.missing(10), [2]);
    }

    #[test]
    fn a_joining_computer_seals_from_the_slot_after_the_save() {
        let mut late = LockstepSession::new(2, 3, 6);
        late.start_at(100, 4);
        assert_eq!(late.lockstep.epoch, 4);
        assert!(
            late.slot(100).is_some_and(|c| c.is_empty()),
            "the save's slot has run"
        );
        late.slot(101);
        let slots: Vec<u64> = late
            .take_outbox()
            .into_iter()
            .filter_map(|m| match m {
                Message::Turn(t) => Some(t.slot),
                _ => None,
            })
            .collect();
        assert_eq!(slots, (101..=107).collect::<Vec<_>>());
    }
}
