//! A lockstep test in one process: players joined by a simulated network, and its report.

use super::*;

/// A lockstep test in one process: several players' games joined by a simulated network.
#[derive(Debug, Clone, Copy)]
pub struct Simulation {
    pub players: u8,
    pub delay: u64,
    /// A message arrives 1 to `1 + jitter` ticks after it is sent, so messages cross and reorder.
    pub jitter: u64,
    /// In each tick a player's computer is too busy to update with this chance in 100.
    pub stall: u64,
    /// A digest every this many updates, for the report.
    pub every: u64,
    /// The players send the host a digest every this many updates (0: no checks).
    pub check: u64,
    /// Scramble this player's game `Random` once this many updates have begun (a desync to find).
    pub corrupt: Option<(u8, u64)>,
    /// The network's random numbers (not the game's).
    pub seed: u64,
}

/// What a simulation, or one player of a networked test, found.
#[derive(Default)]
pub struct Report {
    /// Each player's digests.
    pub digests: Vec<Vec<Digest>>,
    /// The first digest a player disagrees on after the last resync: (player, player 0's, theirs).
    pub mismatch: Option<(u8, Digest, Digest)>,
    /// With one player and no delay, the recording's digests compared and the first that
    /// differs: a lockstep game then carries commands out where a replay does.
    pub recorded: usize,
    pub recorded_mismatch: Option<(Digest, Digest)>,
    /// The updates at which the host found the players disagreeing, and where it resynced them.
    pub desyncs: Vec<u64>,
    pub resyncs: Vec<u64>,
    /// Ticks the simulation took, and the most updates one player was ahead of another.
    pub ticks: u64,
    pub lead: u64,
}

/// A recording's commands split between `players`: whoever starts a tool gives that tool's
/// commands, and the next tool goes to the next player. Also the recording's digests (and pause
/// menu, for a replay), and the last update it needs.
pub struct Script {
    pub inputs: Vec<std::collections::VecDeque<(u64, Option<PlayerCommand>, bool)>>,
    pub recorded: BTreeMap<u64, Digest>,
    pub last: u64,
}

/// Loads `save` once for each player, splits the recording's commands between them
/// ([`Script`]), and runs every game to `updates` (at least the recording's end) over a
/// simulated network ([`SimNet`]). Player 0 is the host.
pub fn simulate(
    game: &mut crate::Game,
    save: &[u8],
    recording: &str,
    sim: Simulation,
    updates: u64,
) -> Result<Report, String> {
    let players = sim.players.max(1) as usize;
    // With one player and no delay the game replays the recording, pause menu included.
    let replay = players == 1 && sim.delay == 0;
    let mut script = Script::new(recording, players, sim.delay, replay)?;
    let last = script.last.max(updates);
    let net = SimNet::new(sim.jitter, sim.seed);
    let mut peers = Vec::new();
    for player in 0..players {
        let saved = crate::saves::saved::SavedGame::parse(save).map_err(|e| e.to_string())?;
        let mut g = GameplayLoop::from_save(saved);
        g.join_lockstep(player as u8, players as u8, sim.delay);
        let link = SimLink {
            net: net.clone(),
            player: player as u8,
            players: players as u8,
        };
        g.join_network(Box::new(link), sim.check);
        peers.push(NetPeer::with_game(player as u8, g));
    }
    // Each town keeps its own shared animation instances, as on its own computer.
    use unbanished_game::behaviour::animation::shared;
    let mut tables: Vec<shared::Table> = (0..players).map(|_| shared::Table::default()).collect();
    game.with_ctx(|_, ctx| {
        for (peer, table) in peers.iter_mut().zip(&mut tables) {
            let g = peer.game.as_mut().unwrap();
            shared::swap(table);
            g.load(ctx);
            g.startup(ctx);
            g.autosave = None;
            g.recording.replaying = replay;
            shared::swap(table);
        }
        let mut report = Report {
            digests: vec![Vec::new(); players],
            ..Default::default()
        };
        let mut corrupt = sim.corrupt;
        let mut idle = 0;
        loop {
            let mut progressed = false;
            for p in 0..players {
                shared::swap(&mut tables[p]);
                let arrived = peers[p].poll(ctx);
                shared::swap(&mut tables[p]);
                progressed |= arrived?;
            }
            for p in 0..players {
                let busy = net.borrow_mut().random(100) < sim.stall;
                let peer = &mut peers[p];
                if peer.game.as_ref().is_none_or(|g| g.sim.updates > last) || busy {
                    continue;
                }
                shared::swap(&mut tables[p]);
                let result = (|| -> Result<bool, String> {
                    script.give(p, peer.game.as_mut().unwrap(), ctx);
                    let Some(update) = peer.begin(ctx) else {
                        return Ok(false);
                    };
                    if let Some((who, at)) = corrupt {
                        if who as usize == p && update >= at {
                            peer.game.as_mut().unwrap().sim.rng =
                                unbanished_game::random::Random::new(0xbad5eed);
                            corrupt = None;
                        }
                    }
                    peer.boundary(ctx, update)?;
                    let g = peer.game.as_mut().unwrap();
                    if sim.every > 0 && update % sim.every == 0
                        || script.recorded.contains_key(&update)
                    {
                        let d = g.digest(ctx, update as u32)?;
                        if replay {
                            if let Some(r) = script.recorded.get(&update) {
                                report.recorded += 1;
                                if r.crc != d.crc && report.recorded_mismatch.is_none() {
                                    report.recorded_mismatch = Some((*r, d));
                                }
                            }
                        }
                        report.digests[p].retain(|old| (old.update as u64) < update);
                        report.digests[p].push(d);
                    }
                    peer.update(ctx);
                    Ok(true)
                })();
                shared::swap(&mut tables[p]);
                progressed |= result?;
            }
            let (low, high) = peers
                .iter()
                .flat_map(|p| p.game.as_ref())
                .fold((u64::MAX, 0), |(lo, hi), g| {
                    (lo.min(g.sim.updates), hi.max(g.sim.updates))
                });
            report.lead = report.lead.max(high - low);
            report.ticks += 1;
            net.borrow_mut().tick += 1;
            let in_flight = net.borrow().in_flight();
            if low > last
                && !in_flight
                && !peers[0].game.as_ref().is_some_and(|g| g.resync_pending())
            {
                break;
            }
            idle = if progressed || in_flight { 0 } else { idle + 1 };
            if idle > 1000 {
                return Err(format!("stuck at updates {low}..{high}"));
            }
        }
        report.desyncs = peers[0].desyncs();
        report.resyncs = peers[0].resyncs();
        report.mismatch = first_mismatch(&report.digests, &report.resyncs);
        Ok(report)
    })
}

/// Every player's digests against player 0's, after the last resync.
pub fn first_mismatch(digests: &[Vec<Digest>], resyncs: &[u64]) -> Option<(u8, Digest, Digest)> {
    let after = resyncs.last().copied().unwrap_or(0);
    for (p, theirs) in digests.iter().enumerate().skip(1) {
        let theirs: BTreeMap<u32, Digest> = theirs.iter().map(|d| (d.update, *d)).collect();
        let first = digests[0]
            .iter()
            .filter(|d| d.update as u64 > after)
            .find_map(|a| theirs.get(&a.update).filter(|b| *b != a).map(|b| (*a, *b)));
        if let Some((a, b)) = first {
            return Some((p as u8, a, b));
        }
    }
    None
}

impl Script {
    pub fn new(
        recording: &str,
        players: usize,
        delay: u64,
        replay: bool,
    ) -> Result<Script, String> {
        use super::super::recording::Entry;
        let mut script = Script {
            inputs: vec![Default::default(); players],
            recorded: BTreeMap::new(),
            last: 0,
        };
        let (mut owner, mut starts) = (0, 0);
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
                    script.last = script.last.max(update + delay + 1);
                    script.inputs[owner].push_back((update, Some(command), false));
                }
                Entry::MenuPause { update, paused } if replay => {
                    script.inputs[0].push_back((update, None, paused));
                }
                Entry::Digest(d) => {
                    script.last = script.last.max(d.update as u64);
                    script.recorded.insert(d.update as u64, d);
                }
                Entry::MenuPause { .. } | Entry::Start { .. } => {}
            }
        }
        Ok(script)
    }

    /// Gives `player`'s commands of the updates begun so far. A player the recording was not
    /// split for (one who joined the running game) gives none.
    pub fn give(&mut self, player: usize, g: &mut GameplayLoop, ctx: &mut Ctx) {
        if player >= self.inputs.len() {
            return;
        }
        while self.inputs[player]
            .front()
            .is_some_and(|(u, _, _)| *u <= g.sim.updates)
        {
            match self.inputs[player].pop_front().unwrap() {
                (_, Some(command), _) => g.give(ctx.archive, command),
                (_, None, paused) => g.menu_paused = paused,
            }
        }
    }
}
