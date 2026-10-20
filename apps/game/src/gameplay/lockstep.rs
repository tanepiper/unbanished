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
}

impl Lockstep {
    pub fn new(player: u8, players: u8, delay: u64) -> Lockstep {
        Lockstep {
            player,
            players,
            delay,
            local: BTreeMap::new(),
            turns: BTreeMap::new(),
        }
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

    /// The commands of `slot`, by player, then in the order given; the slot is then forgotten.
    pub fn take(&mut self, slot: u64) -> Vec<(u8, PlayerCommand)> {
        self.turns
            .remove(&slot)
            .unwrap_or_default()
            .into_iter()
            .flat_map(|(player, commands)| commands.into_iter().map(move |c| (player, c)))
            .collect()
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

    /// The turns sealed since the last call, to send to the other players.
    pub fn take_outbox(&mut self) -> Vec<Turn> {
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
        if self.sealed != Some(slot) {
            self.sealed = Some(slot);
            let turn = l.seal(slot);
            self.outbox.push(turn);
        }
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

/// A lockstep test in one process: several players' games joined by a simulated network.
#[derive(Debug, Clone, Copy)]
pub struct Simulation {
    pub players: u8,
    pub delay: u64,
    /// A turn arrives 1 to `1 + jitter` ticks after it is sent, so turns cross and reorder.
    pub jitter: u64,
    /// In each tick a player's computer is too busy to update with this chance in 100.
    pub stall: u64,
    /// A digest every this many updates.
    pub every: u64,
    /// The network's random numbers (not the game's).
    pub seed: u64,
}

/// What a simulation found.
pub struct Report {
    /// Each player's digests.
    pub digests: Vec<Vec<super::digest::Digest>>,
    /// The first digest a player disagrees on: (player, player 0's, theirs).
    pub mismatch: Option<(u8, super::digest::Digest, super::digest::Digest)>,
    /// With one player and no delay, the recording's digests compared and the first that
    /// differs: a lockstep game then carries commands out where a replay does.
    pub recorded: usize,
    pub recorded_mismatch: Option<(super::digest::Digest, super::digest::Digest)>,
    /// Ticks the simulation took, and the most updates one player was ahead of another.
    pub ticks: u64,
    pub lead: u64,
}

/// Loads `save` once for each player, splits the recording's commands between them (whoever
/// starts a tool gives that tool's commands; the next tool goes to the next player), and runs
/// every game to `updates` (at least the recording's end) through a simulated network.
pub fn simulate(
    game: &mut crate::Game,
    save: &[u8],
    recording: &str,
    sim: Simulation,
    updates: u64,
) -> Result<Report, String> {
    use super::recording::Entry;
    use std::collections::VecDeque;
    let players = sim.players.max(1) as usize;
    // With one player and no delay the game replays the recording, pause menu included.
    let replay = players == 1 && sim.delay == 0;
    let mut inputs: Vec<VecDeque<(u64, Option<PlayerCommand>, bool)>> =
        vec![VecDeque::new(); players];
    let mut recorded = BTreeMap::new();
    let mut owner = 0;
    let mut starts = 0;
    let mut last = updates;
    for (i, line) in recording.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let entry: Entry =
            serde_json::from_str(line).map_err(|e| format!("line {}: {e}", i + 1))?;
        match entry {
            Entry::Command { update, command } => {
                if matches!(command, PlayerCommand::StartTool { .. }) {
                    owner = starts % players;
                    starts += 1;
                }
                last = last.max(update + sim.delay + 1);
                inputs[owner].push_back((update, Some(command), false));
            }
            Entry::MenuPause { update, paused } if replay => {
                inputs[0].push_back((update, None, paused));
            }
            Entry::Digest(d) => {
                last = last.max(d.update as u64);
                recorded.insert(d.update as u64, d);
            }
            Entry::MenuPause { .. } | Entry::Start { .. } => {}
        }
    }
    let step = banished_game::scheduler::seconds(crate::FRAME_STEP);
    let mut peers = Vec::new();
    for player in 0..players {
        let saved = crate::saves::saved::SavedGame::parse(save).map_err(|e| e.to_string())?;
        let mut g = super::GameplayLoop::from_save(saved);
        g.join_lockstep(player as u8, players as u8, sim.delay);
        peers.push(g);
    }
    let mut seed = sim.seed | 1;
    let mut random = move |n: u64| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        if n == 0 {
            0
        } else {
            seed % n
        }
    };
    // Each town keeps its own shared animation instances, as on its own computer.
    use banished_game::behaviour::animation::shared;
    let mut tables: Vec<shared::Table> = (0..players).map(|_| shared::Table::default()).collect();
    game.with_ctx(|_, ctx| {
        for (g, table) in peers.iter_mut().zip(&mut tables) {
            shared::swap(table);
            g.load(ctx);
            g.startup(ctx);
            g.autosave = None;
            g.replaying = replay;
            shared::swap(table);
        }
        let mut report = Report {
            digests: vec![Vec::new(); players],
            mismatch: None,
            recorded: 0,
            recorded_mismatch: None,
            ticks: 0,
            lead: 0,
        };
        let mut network: Vec<(u64, usize, Turn)> = Vec::new();
        let mut idle = 0;
        loop {
            let tick = report.ticks;
            let mut arrived = Vec::new();
            network.retain(|(at, to, turn)| {
                let due = *at <= tick;
                if due {
                    arrived.push((*to, turn.clone()));
                }
                !due
            });
            for (to, turn) in arrived {
                peers[to].receive_turn(turn);
            }
            let mut progressed = false;
            for p in 0..players {
                if peers[p].updates > last || random(100) < sim.stall {
                    continue;
                }
                shared::swap(&mut tables[p]);
                while inputs[p]
                    .front()
                    .is_some_and(|(u, _, _)| *u <= peers[p].updates)
                {
                    match inputs[p].pop_front().unwrap() {
                        (_, Some(command), _) => peers[p].give(ctx.archive, command),
                        (_, None, paused) => peers[p].menu_paused = paused,
                    }
                }
                let ready = peers[p].lockstep_slot(ctx);
                for turn in peers[p].take_outbox() {
                    for q in (0..players).filter(|&q| q != p) {
                        network.push((tick + 1 + random(sim.jitter + 1), q, turn.clone()));
                    }
                }
                if !ready {
                    shared::swap(&mut tables[p]);
                    continue;
                }
                let update = peers[p].updates;
                if sim.every > 0 && update % sim.every == 0 || recorded.contains_key(&update) {
                    let d = peers[p].digest(ctx, update as u32)?;
                    if players == 1 && sim.delay == 0 {
                        if let Some(r) = recorded.get(&update) {
                            report.recorded += 1;
                            if r.crc != d.crc && report.recorded_mismatch.is_none() {
                                report.recorded_mismatch = Some((*r, d));
                            }
                        }
                    }
                    report.digests[p].push(d);
                }
                peers[p].update(ctx, step);
                shared::swap(&mut tables[p]);
                progressed = true;
            }
            let (low, high) = peers.iter().fold((u64::MAX, 0), |(lo, hi), g| {
                (lo.min(g.updates), hi.max(g.updates))
            });
            report.lead = report.lead.max(high - low);
            report.ticks += 1;
            if low > last {
                break;
            }
            idle = if progressed || !network.is_empty() {
                0
            } else {
                idle + 1
            };
            if idle > 1000 {
                return Err(format!("stuck at updates {low}..{high}"));
            }
        }
        // Every player's digests against player 0's.
        for p in 1..players {
            for (a, b) in report.digests[0].iter().zip(&report.digests[p]) {
                if a != b {
                    report.mismatch = Some((p as u8, *a, *b));
                    break;
                }
            }
            if report.mismatch.is_some() {
                break;
            }
        }
        Ok(report)
    })
}
