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
    /// `absent`: players numbered by the relay who left before the start.
    Start {
        players: u8,
        delay: u64,
        check: u64,
        absent: Vec<u8>,
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
    /// Players who left, and the first slot without them.
    left: BTreeMap<u8, u64>,
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
            left: BTreeMap::new(),
        }
    }

    /// `player` left: their turns are wanted up to the last that arrived, and none after. Every
    /// computer has the same turns from them (the relay passes on all of a player's data
    /// before the news that they left, and each computer gets its messages in order), so all
    /// agree on the slot. Returns that slot, and the host's checks that are now complete.
    pub fn leave(&mut self, player: u8) -> (u64, Vec<(u64, bool)>) {
        if let Some(&from) = self.left.get(&player) {
            return (from, Vec::new());
        }
        let last = self
            .turns
            .iter()
            .chain(self.history.iter())
            .filter(|(_, t)| t.contains_key(&player))
            .map(|(&s, _)| s)
            .max();
        let from = last.map_or(0, |s| s + 1);
        self.left.insert(player, from);
        let complete: Vec<u64> = self
            .checks
            .iter()
            .filter(|(_, held)| self.all_in(held))
            .map(|(&u, _)| u)
            .collect();
        let results = complete.into_iter().map(|u| (u, self.agree(u))).collect();
        (from, results)
    }

    /// The players who left, and the first slot without each.
    pub fn departed(&self) -> &BTreeMap<u8, u64> {
        &self.left
    }

    /// Every player still here has sent a digest.
    fn all_in(&self, held: &BTreeMap<u8, u32>) -> bool {
        (0..self.players).all(|p| held.contains_key(&p) || self.left.contains_key(&p))
    }

    fn agree(&mut self, update: u64) -> bool {
        let held = self.checks.remove(&update).unwrap_or_default();
        let first = held.values().next().copied();
        held.values().all(|&c| Some(c) == first)
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
        self.checks.entry(update).or_default().insert(player, crc);
        if !self.all_in(&self.checks[&update]) {
            return None;
        }
        Some(self.agree(update))
    }

    /// The local player gives a command while `updates` updates have begun. While the game
    /// waits for the others, that slot's turn is already sealed and sent: the command goes in
    /// the first slot not sealed yet.
    pub fn give(&mut self, updates: u64, command: PlayerCommand) {
        let open = self.sealed.map_or(0, |s| s + 1);
        self.local
            .entry(updates.max(open) + self.delay)
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
        if slot < self.delay {
            return true;
        }
        let turns = self.turns.get(&slot);
        (0..self.players).all(|p| {
            turns.is_some_and(|t| t.contains_key(&p))
                || self.left.get(&p).is_some_and(|&from| slot >= from)
        })
    }

    /// The players whose turn for `slot` has not arrived.
    pub fn missing(&self, slot: u64) -> Vec<u8> {
        if slot < self.delay {
            return Vec::new();
        }
        let turns = self.turns.get(&slot);
        (0..self.players)
            .filter(|p| !turns.is_some_and(|t| t.contains_key(p)))
            .filter(|p| self.left.get(p).is_none_or(|&from| slot < from))
            .collect()
    }

    /// The commands of `slot`, by player, then in the order given; the slot moves to the
    /// history. A turn of a player who left that arrives late is not used.
    pub fn take(&mut self, slot: u64) -> Vec<(u8, PlayerCommand)> {
        let mut turns = self.turns.remove(&slot).unwrap_or_default();
        turns.retain(|p, _| self.left.get(p).is_none_or(|&from| slot < from));
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
    fn a_player_who_leaves_is_not_waited_for_after_their_last_turn() {
        let mut a = Lockstep::new(0, 3, 2);
        let mut c = Lockstep::new(2, 3, 2);
        // Player 1 sent turns for slots 2 and 3, then left.
        for slot in [2, 3] {
            let t = Turn {
                player: 1,
                slot,
                commands: vec![speed(slot as u8)],
            };
            a.receive(t.clone());
            c.receive(t);
        }
        for slot in 2..6 {
            let (ta, tc) = (a.seal(slot - 2), c.seal(slot - 2));
            a.receive(tc);
            c.receive(ta);
        }
        assert!(a.ready(3) && !a.ready(4), "slot 4 waits for player 1");
        assert_eq!(a.leave(1).0, 4);
        assert_eq!(c.leave(1).0, 4);
        for slot in 2..6 {
            assert!(a.ready(slot) && c.ready(slot));
            assert_eq!(format!("{:?}", a.take(slot)), format!("{:?}", c.take(slot)));
        }
        // The host's check waiting for player 1 completes without them.
        let mut host = Lockstep::new(0, 3, 2);
        assert_eq!(host.check(0, 60, 7), None);
        assert_eq!(host.check_in(2, 0, 60, 7), None);
        assert_eq!(host.leave(1).1, vec![(60, true)]);
        assert_eq!(host.check(0, 120, 7), None);
        assert_eq!(host.check_in(2, 0, 120, 8), Some(false));
    }

    #[test]
    fn a_command_given_while_waiting_goes_in_the_next_turn() {
        let mut a = Lockstep::new(0, 2, 3);
        // Update 5 waits: its turn (slot 8) is sealed, the other player's slot 5 is missing.
        let sealed = a.seal_through(5);
        assert_eq!(sealed.last().map(|t| t.slot), Some(8));
        a.give(5, speed(4));
        let next = a.seal_through(6);
        assert_eq!(next.len(), 1);
        assert_eq!((next[0].slot, next[0].commands.len()), (9, 1));
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
    pub fn give(&mut self, archive: &unbanished_formats::pkg::Archive, command: PlayerCommand) {
        match &mut self.lockstep {
            Some(l) => l.give(self.sim.updates, command),
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
        let slot = self.sim.updates;
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
        let commands = l.take(slot);
        self.carrying_out = true;
        for (player, command) in commands {
            self.execute_from(ctx, player, command);
        }
        self.carrying_out = false;
        self.slot_done = Some(slot);
        true
    }

    /// Carries out a player's command. A tool it starts is the local player's current tool, or
    /// kept for the other player (their Path tool; drag tools are kept by template for all).
    fn execute_from(&mut self, ctx: &mut crate::loops::Ctx, player: u8, command: PlayerCommand) {
        use super::commands::{Outcome, Tool};
        let local = self.lockstep.as_ref().is_none_or(|l| l.player == player);
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
        g.outbox = std::mem::take(&mut self.outbox);
        g.net = self.net.take();
        if let Some(net) = &mut g.net {
            net.forget_line();
        }
        g.sim.updates = update;
        g.slot_done = Some(update);
        g.boundary_done = Some(update);
        if let Some(mut l) = self.lockstep.take() {
            l.rewind(update);
            l.epoch += 1;
            l.checks.clear();
            let sealed = l.seal_through(update);
            g.outbox.extend(sealed.into_iter().map(Message::Turn));
            g.lockstep = Some(l);
        }
        self.retired = true;
        Ok(g)
    }
}
