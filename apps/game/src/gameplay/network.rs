//! The players' games over a network: a [`Link`] to the other players, and a [`NetPeer`] that
//! drives one player's lockstep game over it (turns, the host's digest checks and resyncs, the
//! start from the host's save). The link is the relay ([`RelayLink`], `unbanished-net`), or a
//! simulated network in one process for tests ([`simulate`]). See docs/MULTIPLAYER-PLAN.md,
//! steps 3 to 5.
//!
//! Deviation: not in the original, which has no multiplayer.
// The browser joins through the same code once a page asks for it; until then parts are unused.
#![cfg_attr(target_arch = "wasm32", allow(dead_code))]

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use super::commands::PlayerCommand;
use super::digest::Digest;
use super::lockstep::Message;
use super::GameplayLoop;
use crate::loops::{Ctx, GameLoop};
pub use unbanished_session::link::{Link, RelayLink, Shared, SimLink, SimNet};

/// A game's side of a multiplayer session: the link to the other players and the host's
/// checks. It moves with the game across a resync.
pub struct NetState {
    pub link: Box<dyn Link>,
    /// The players send the host a digest every this many updates (0: none).
    pub check: u64,
    resync_wanted: bool,
    /// The updates at which the host found the players disagreeing, and resynced them.
    pub desyncs: Vec<u64>,
    pub resyncs: Vec<u64>,
    /// Messages that arrived behind a resync's save, for the reloaded game.
    inbox: std::collections::VecDeque<(u8, Message)>,
    /// A [`NetPeer`] drives the session step by step, not the game's own update.
    pub driven: bool,
    /// The players line (its text element), and the updates in a row spent waiting for the
    /// other players' turns.
    line: Option<unbanished_ui::NodeId>,
    waiting: u32,
    /// The line's text as last set.
    shown: String,
    /// Players who left, not yet taken out of the lockstep (a resync's save came first).
    leaving: Vec<u8>,
    /// The host: players joining the running game, and the update to send each its save at.
    joining: Vec<(u8, u64)>,
    /// Whether the server may keep this room's town: the host gives it a save every
    /// [`KEEP_EVERY`] updates until the server says it keeps none.
    keeps: bool,
}

/// How often the host gives a server that keeps rooms its town: every minute of play.
const KEEP_EVERY: u64 = 3600;

impl NetState {
    /// The players line belongs to the screen it was on.
    pub(super) fn forget_line(&mut self) {
        self.line = None;
        self.waiting = 0;
        self.shown.clear();
    }

    /// The server's check of `update`: the same log lines as the host's own check, and a
    /// difference among the desyncs (the server picks who resyncs).
    fn noted(&mut self, update: u64, agree: bool) {
        if agree {
            log::info!("multiplayer: the players agree at update {update}");
        } else {
            log::warn!("multiplayer: the players differ at update {update}");
            self.desyncs.push(update);
        }
    }

    /// The host's check of `update`, once every player's digest is in.
    fn checked(&mut self, update: u64, agree: Option<bool>) {
        match agree {
            Some(true) => log::info!("multiplayer: the players agree at update {update}"),
            Some(false) => {
                log::warn!("multiplayer: the players differ at update {update}");
                self.desyncs.push(update);
                self.resync_wanted = true;
            }
            None => {}
        }
    }
}

/// Who this game is, for the relay: the protocol, the engine's version, and fingerprints of the
/// game data and the mods (`Archive::fingerprints`).
pub fn hello(archive: &unbanished_formats::pkg::Archive) -> unbanished_net::Hello {
    let (data, mods) = archive.fingerprints();
    unbanished_net::Hello::new(crate::platform::version::NAME_VERSION, data, mods)
        .with_hooks(crate::platform::rule_hooks::fingerprint())
}

impl GameplayLoop {
    /// The game the host's [`Message::Start`] starts for `player`: its save, the lockstep (for
    /// a player joining a running game, at the save's update and epoch) and the network.
    /// `early` are the messages that came before the start (turns of a running game).
    pub fn from_start(
        player: u8,
        start: Message,
        link: Box<dyn Link>,
        early: Vec<(u8, Message)>,
    ) -> Result<GameplayLoop, String> {
        let Message::Start {
            players,
            delay,
            check,
            absent,
            bytes,
            update,
            epoch,
            speed,
            paused,
            tools,
        } = start
        else {
            return Err("not a start".into());
        };
        let saved = crate::saves::saved::SavedGame::parse(&bytes).map_err(|e| e.to_string())?;
        let mut g = GameplayLoop::from_save(saved);
        g.start_lockstep(player, players, delay, &absent);
        if update > 0 {
            if let Some(l) = g.online.lockstep.as_mut() {
                l.start_at(update, epoch);
            }
            g.sim.updates = update;
            g.online.boundary_done = Some(update);
            g.speed = speed;
            g.paused = paused;
            g.online.tools = tools;
        }
        g.join_network(link, check);
        // The others reload the same save as a resync: this player started from it already.
        let early = early
            .into_iter()
            .filter(|(_, m)| !matches!(m, Message::Save { update: u, .. } if *u <= update));
        g.online.net.as_mut().unwrap().inbox.extend(early);
        Ok(g)
    }
}

/// A resync's save and the update it was made at: the game is to be replaced by it.
pub type Reload = (Vec<u8>, u64);

impl GameplayLoop {
    /// Plays this (lockstep) game over `link`.
    pub fn join_network(&mut self, mut link: Box<dyn Link>, check: u64) {
        // Players who came before the start are in it; only later ones join a running game.
        link.joined();
        self.online.net = Some(NetState {
            link,
            check,
            resync_wanted: false,
            desyncs: Vec::new(),
            resyncs: Vec::new(),
            inbox: Default::default(),
            driven: false,
            line: None,
            waiting: 0,
            shown: String::new(),
            leaving: Vec::new(),
            joining: Vec::new(),
            keeps: true,
        });
    }

    /// Plays in lockstep as `player` of `players`; the `absent` players left before the
    /// start.
    pub fn start_lockstep(&mut self, player: u8, players: u8, delay: u64, absent: &[u8]) {
        self.join_lockstep(player, players, delay);
        if let Some(l) = self.online.lockstep.as_mut().map(|s| &mut s.lockstep) {
            for &p in absent {
                l.leave(p);
            }
        }
    }

    /// The session, if this is a multiplayer game.
    pub fn net(&self) -> Option<&NetState> {
        self.online.net.as_ref()
    }

    /// Handles what arrived: turns, and (the host) the players' digests. A resync's save stops
    /// it and is returned; what came after it waits for the reloaded game. True when anything
    /// arrived.
    pub fn net_poll(&mut self) -> (bool, Option<Reload>) {
        let Some(net) = &mut self.online.net else {
            return (false, None);
        };
        let mut arrived: std::collections::VecDeque<(u8, Message)> = net.inbox.drain(..).collect();
        arrived.extend(net.link.poll());
        let left = net.link.left();
        net.leaving.extend(left);
        let any = !arrived.is_empty();
        while let Some((_, message)) = arrived.pop_front() {
            match message {
                Message::Turn(turn) => self.receive_turn(turn),
                Message::Digest {
                    player,
                    epoch,
                    update,
                    crc,
                } => {
                    let l = self.online.lockstep.as_mut().map(|s| &mut s.lockstep);
                    let agree = l.and_then(|l| l.check_in(player, epoch, update, crc));
                    self.online.net.as_mut().unwrap().checked(update, agree);
                }
                // The server found the games different, and this one is to be kept.
                Message::SaveWanted { update } => {
                    let net = self.online.net.as_mut().unwrap();
                    net.resync_wanted = true;
                    log::info!("multiplayer: the server keeps this game from update {update}");
                }
                // The server's check: logged, and a difference noted; the server asks the
                // player whose game is kept for its save.
                Message::Checked { update, agree } => {
                    self.online.net.as_mut().unwrap().noted(update, agree);
                }
                Message::Save { update, bytes } => {
                    self.online.net.as_mut().unwrap().inbox = arrived;
                    return (true, Some((bytes, update)));
                }
                Message::Start { .. } => log::warn!("a start in a game already started"),
                // A player joins the running game: its turns count from slot `from`.
                Message::Grow { player, from } => {
                    if let Some(l) = self.online.lockstep.as_mut() {
                        l.lockstep.add(player, from);
                    }
                    log::info!("multiplayer: player {} joins from slot {from}", player + 1);
                }
            }
        }
        // Who left, once every turn they sent is in.
        let leaving = std::mem::take(&mut self.online.net.as_mut().unwrap().leaving);
        for player in leaving {
            let Some(l) = self.online.lockstep.as_mut().map(|s| &mut s.lockstep) else {
                continue;
            };
            let (from, checks) = l.leave(player);
            log::info!(
                "multiplayer: player {} left; the game goes on from slot {from}",
                player + 1
            );
            let net = self.online.net.as_mut().unwrap();
            for (update, agree) in checks {
                net.checked(update, Some(agree));
            }
        }
        // Who joined the running game: the host brings them in (the others hear it in `Grow`).
        let updates = self.sim.updates;
        let net = self.online.net.as_mut().unwrap();
        let joined = net.link.joined();
        if let Some(l) = self
            .online
            .lockstep
            .as_mut()
            .filter(|l| l.lockstep.player == 0)
        {
            for player in joined {
                // Every turn for slot `from` on is sealed after the newcomer was in the room:
                // a player seals slot S during update S - delay, which it reaches only after
                // the host passed update S - 2 delay - 1. And no player has run slot `from`
                // yet: none is more than `delay` updates ahead of the host.
                let from = updates + 2 * l.lockstep.delay + 2;
                l.lockstep.add(player, from);
                net.link.send(None, &Message::Grow { player, from });
                net.joining.push((player, from - 1));
                log::info!("multiplayer: player {} joins from slot {from}", player + 1);
            }
        }
        (any, None)
    }

    /// Sends the sealed turns.
    pub fn net_send(&mut self) {
        let outbox = self.take_outbox();
        if let Some(net) = &mut self.online.net {
            for message in outbox {
                net.link.send(None, &message);
            }
        }
    }

    /// At an update boundary (once): the host resyncs everybody if a check failed (the save is
    /// sent, and returned for its own game), then every player sends the host its digest when
    /// one is due.
    pub fn net_boundary(&mut self, ctx: &mut Ctx, update: u64) -> Result<Option<Reload>, String> {
        if self.online.net.is_none() || self.online.boundary_done == Some(update) {
            return Ok(None);
        }
        self.online.boundary_done = Some(update);
        let host = self
            .online
            .lockstep
            .as_ref()
            .is_some_and(|l| l.lockstep.player == 0);
        // The host resyncs when its own check failed; any player when the server asked it.
        if self.online.net.as_ref().unwrap().resync_wanted {
            let bytes = self
                .save_file(ctx.archive, ctx.ui, ctx.mods, None, None)
                .ok_or("this player could not save")?;
            let net = self.online.net.as_mut().unwrap();
            net.resync_wanted = false;
            net.resyncs.push(update);
            let save = Message::Save {
                update,
                bytes: bytes.clone(),
            };
            net.link.send(None, &save);
            if host {
                net.link.keep(update, &bytes);
            }
            return Ok(Some((bytes, update)));
        }
        // A player joining the running game gets the save of the update before its first slot,
        // as its start.
        let due: Vec<u8> = {
            let net = self.online.net.as_mut().unwrap();
            let due = net
                .joining
                .iter()
                .filter(|&&(_, at)| at == update)
                .map(|&(p, _)| p)
                .collect();
            net.joining.retain(|&(_, at)| at > update);
            due
        };
        if !due.is_empty() {
            // Everybody goes on from one save: the others reload it (a resync), and the newcomer
            // starts from it. So every town has its templates in the same order.
            let bytes = self
                .save_file(ctx.archive, ctx.ui, ctx.mods, None, None)
                .ok_or("the host could not save for a joining player")?;
            let l = &self.online.lockstep.as_ref().ok_or("no lockstep")?.lockstep;
            let start = Message::Start {
                players: l.players,
                delay: l.delay,
                check: self.online.net.as_ref().unwrap().check,
                absent: l.absent(),
                bytes: bytes.clone(),
                update,
                epoch: l.epoch + 1,
                speed: self.speed,
                paused: self.paused,
                tools: self.sim.drag_tools.keys().cloned().collect(),
            };
            let net = self.online.net.as_mut().unwrap();
            let save = Message::Save {
                update,
                bytes: bytes.clone(),
            };
            net.link.send(None, &save);
            for player in due {
                net.link.send(Some(player), &start);
                log::info!(
                    "multiplayer: player {} starts at update {update}",
                    player + 1
                );
            }
            net.link.keep(update, &bytes);
            return Ok(Some((bytes, update)));
        }
        // A server that keeps rooms gets the host's town every minute of play.
        if host && update > 0 && update % KEEP_EVERY == 0 {
            let keeps = self.online.net.as_ref().is_some_and(|n| n.keeps);
            if keeps {
                let bytes = self
                    .save_file(ctx.archive, ctx.ui, ctx.mods, None, None)
                    .ok_or("the host could not save its town to keep")?;
                let net = self.online.net.as_mut().unwrap();
                net.keeps = net.link.keep(update, &bytes);
            }
        }
        let check = self.online.net.as_ref().unwrap().check;
        if check > 0 && update > 0 && update % check == 0 {
            let crc = self.digest(ctx, update as u32)?.crc;
            let l = &mut self.online.lockstep.as_mut().ok_or("no lockstep")?.lockstep;
            let (player, epoch) = (l.player, l.epoch);
            // A server that checks the games gets every player's digest, the host's too.
            if self
                .online
                .net
                .as_mut()
                .unwrap()
                .link
                .send_digest(epoch, update, crc)
            {
                return Ok(None);
            }
            let l = &mut self.online.lockstep.as_mut().ok_or("no lockstep")?.lockstep;
            let agree = (player == 0).then(|| l.check(0, update, crc)).flatten();
            let net = self.online.net.as_mut().unwrap();
            if player == 0 {
                net.checked(update, agree);
            } else {
                let message = Message::Digest {
                    player,
                    epoch,
                    update,
                    crc,
                };
                net.link.send(Some(0), &message);
            }
        }
        Ok(None)
    }

    /// Deviation: the players line, at the foot of the screen in a multiplayer game. It is the
    /// original's version footer (`Dialog/BuildId.rsc`) without its logo.
    pub(super) fn open_players_line(&mut self, ui: &mut unbanished_ui::Ui) {
        let (Some(net), Some(screen)) = (self.online.net.as_mut(), self.screen.as_mut()) else {
            return;
        };
        let Some(d) = screen.add_dialog(ui, "Dialog/BuildId.rsc:resource") else {
            return;
        };
        if let Some(logo) = screen.find(ui, d, "imageLogo") {
            ui.set_visible(logo, false);
        }
        if let Some(title) = screen.find(ui, d, "textBuild") {
            ui.set_text(title, "Multiplayer");
        }
        net.line = screen.find(ui, d, "textBuildNotice");
        net.waiting = 0;
        self.refresh_players_line(ui, false);
    }

    /// The line's text: who this player is, or that the game waits for the others.
    pub(super) fn refresh_players_line(&mut self, ui: &mut unbanished_ui::Ui, waiting: bool) {
        let (Some(net), Some(l)) = (
            self.online.net.as_mut(),
            self.online.lockstep.as_ref().map(|s| &s.lockstep),
        ) else {
            return;
        };
        net.waiting = if waiting {
            net.waiting.saturating_add(1)
        } else {
            0
        };
        let Some(line) = net.line else {
            return;
        };
        // Waiting a moment (half a second) is normal; longer is shown.
        let mut text = if net.waiting >= 30 {
            let missing: Vec<String> = l
                .missing(self.sim.updates)
                .iter()
                .map(|p| (p + 1).to_string())
                .collect();
            let who = if missing.len() == 1 {
                "player"
            } else {
                "players"
            };
            format!("Waiting for {who} {}", missing.join(", "))
        } else {
            let host = if l.player == 0 { ", the host" } else { "" };
            format!("Player {} of {}{host}", l.player + 1, l.players)
        };
        let gone = l.departed();
        if !gone.is_empty() {
            let names: Vec<String> = gone.keys().map(|p| (p + 1).to_string()).collect();
            text += &match names.len() {
                1 => format!(". Player {} has left", names[0]),
                _ => format!(". Players {} have left", names.join(", ")),
            };
            if gone.contains_key(&0) {
                text += "; without the host the games are not compared";
            }
        }
        if text.starts_with("Waiting") != net.shown.starts_with("Waiting") {
            log::info!("multiplayer: update {}: {text}", self.sim.updates);
        }
        if text != net.shown {
            ui.set_text(line, &text);
            net.shown = text;
        }
    }

    /// The host found a desync it has not resynced yet.
    pub fn resync_pending(&self) -> bool {
        self.online.net.as_ref().is_some_and(|n| n.resync_wanted)
    }
}

/// One player's game in a multiplayer session, driven a step at a time (the tests, and the
/// browser's network page). Player 0 is the host: it compares the players' digests and resyncs
/// everybody from its save when they differ.
pub struct NetPeer {
    pub player: u8,
    /// The game, once started (a joining player waits for the host's [`Message::Start`]).
    pub game: Option<GameplayLoop>,
    /// The link until the game starts; then the game holds it.
    link: Option<Box<dyn Link>>,
    check: u64,
    /// What arrived before the start (a running game's turns, for a player who joins it).
    early: Vec<(u8, Message)>,
}

impl NetPeer {
    pub fn new(player: u8, link: Box<dyn Link>, check: u64) -> NetPeer {
        NetPeer {
            player,
            game: None,
            link: Some(link),
            check,
            early: Vec::new(),
        }
    }

    /// A peer whose game is already loaded and joined to its network (the in-process test).
    pub fn with_game(player: u8, mut game: GameplayLoop) -> NetPeer {
        if let Some(net) = &mut game.online.net {
            net.driven = true;
        }
        NetPeer {
            player,
            game: Some(game),
            link: None,
            check: 0,
            early: Vec::new(),
        }
    }

    /// The host starts the game for `players` from its save: everybody, the host included,
    /// loads the same bytes.
    pub fn host_start(
        &mut self,
        ctx: &mut Ctx,
        bytes: Vec<u8>,
        players: u8,
        delay: u64,
    ) -> Result<(), String> {
        // A server that keeps rooms keeps the town from its start.
        if let Some(link) = &mut self.link {
            link.keep(0, &bytes);
        }
        let start = Message::Start {
            players,
            delay,
            check: self.check,
            absent: Vec::new(),
            bytes,
            update: 0,
            epoch: 0,
            speed: 0,
            paused: false,
            tools: Vec::new(),
        };
        if let Some(link) = &mut self.link {
            link.send(None, &start);
        }
        self.start(ctx, start, Vec::new())
    }

    fn start(
        &mut self,
        ctx: &mut Ctx,
        start: Message,
        early: Vec<(u8, Message)>,
    ) -> Result<(), String> {
        let link = self.link.take().ok_or("no link")?;
        let mut g = GameplayLoop::from_start(self.player, start, link, early)?;
        g.load(ctx);
        g.startup(ctx);
        g.autosave = None;
        g.online.net.as_mut().unwrap().driven = true;
        self.game = Some(g);
        log::info!("player {} started", self.player);
        Ok(())
    }

    /// Handles what arrived. True when anything did.
    pub fn poll(&mut self, ctx: &mut Ctx) -> Result<bool, String> {
        let Some(g) = &mut self.game else {
            let Some(link) = &mut self.link else {
                return Ok(false);
            };
            let arrived = link.poll();
            let any = !arrived.is_empty();
            let mut arrived = arrived.into_iter();
            while let Some((from, message)) = arrived.next() {
                if matches!(message, Message::Start { .. }) {
                    // What came before the start (a running game's turns), and after it.
                    let mut early = std::mem::take(&mut self.early);
                    early.extend(arrived);
                    self.start(ctx, message, early)?;
                    break;
                }
                self.early.push((from, message));
            }
            return Ok(any);
        };
        let (any, reload) = g.net_poll();
        if let Some((bytes, update)) = reload {
            let old = self.game.take().unwrap();
            self.game = Some(old.reloaded(ctx, &bytes, update)?);
        }
        Ok(any)
    }

    /// Before an update: seals and sends the local turns, and carries the slot out if every
    /// player's turn is here. The updates begun, or none while waiting.
    pub fn begin(&mut self, ctx: &mut Ctx) -> Option<u64> {
        let g = self.game.as_mut()?;
        let ready = g.lockstep_slot(ctx);
        g.net_send();
        ready.then_some(g.sim.updates)
    }

    /// At an update boundary: the host's resync, then the digest checks.
    pub fn boundary(&mut self, ctx: &mut Ctx, update: u64) -> Result<(), String> {
        let g = self.game.as_mut().ok_or("no game")?;
        if let Some((bytes, update)) = g.net_boundary(ctx, update)? {
            let old = self.game.take().unwrap();
            self.game = Some(old.reloaded(ctx, &bytes, update)?);
        }
        Ok(())
    }

    /// The update itself.
    pub fn update(&mut self, ctx: &mut Ctx) {
        let step = unbanished_game::scheduler::seconds(crate::FRAME_STEP);
        if let Some(g) = &mut self.game {
            g.update(ctx, step);
        }
    }

    /// The host's desyncs found and resyncs made.
    pub fn desyncs(&self) -> Vec<u64> {
        self.game
            .as_ref()
            .and_then(|g| g.net())
            .map_or(Vec::new(), |n| n.desyncs.clone())
    }

    pub fn resyncs(&self) -> Vec<u64> {
        self.game
            .as_ref()
            .and_then(|g| g.net())
            .map_or(Vec::new(), |n| n.resyncs.clone())
    }
}

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

impl Script {
    pub fn new(
        recording: &str,
        players: usize,
        delay: u64,
        replay: bool,
    ) -> Result<Script, String> {
        use super::recording::Entry;
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

impl GameplayLoop {
    /// Updates begun since the game loaded.
    pub fn updates(&self) -> u64 {
        self.sim.updates
    }

    /// A test's desync: the game's `Random` scrambled.
    pub fn scramble(&mut self) {
        self.sim.rng = unbanished_game::random::Random::new(0xbad5eed);
    }

    /// [`super::digest`]'s digest of the game now.
    pub fn state_digest(&self, ctx: &mut Ctx, update: u64) -> Result<Digest, String> {
        self.digest(ctx, update as u32)
    }
}

/// A joining player run a little at a time, for the browser's network test page
/// (`?net=RELAY&room=CODE&updates=N`): it joins the room, waits for the host's start, gives no
/// commands of its own, and keeps a digest every 60 updates to compare with the other players'.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub struct JoinSession {
    link: Rc<RefCell<RelayLink>>,
    peer: Option<NetPeer>,
    target: u64,
    /// Its own shared animation instances, apart from the menu's behind the page.
    table: unbanished_game::behaviour::animation::shared::Table,
    pub lines: Vec<String>,
    pub status: String,
    pub done: bool,
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
impl JoinSession {
    pub fn new(relay: &str, room: &str, target: u64, hello: unbanished_net::Hello) -> JoinSession {
        let mut link = RelayLink::new(unbanished_net::Connection::connect(relay), hello);
        link.request(unbanished_net::ToRelay::Join {
            room: room.to_string(),
            password: None,
        });
        JoinSession {
            link: Rc::new(RefCell::new(link)),
            peer: None,
            target,
            table: Default::default(),
            lines: Vec::new(),
            status: format!("joining {room} at {relay}"),
            done: false,
        }
    }

    /// Runs for about `budget` (one browser frame's worth); false once finished.
    pub fn step(&mut self, game: &mut crate::Game, budget: std::time::Duration) -> bool {
        if self.done {
            return false;
        }
        if self.peer.is_none() {
            let _ = Link::poll(&mut *self.link.borrow_mut());
            let events: Vec<_> = self.link.borrow_mut().events.drain(..).collect();
            for e in events {
                match e {
                    unbanished_net::FromRelay::Joined { player, .. } => {
                        let link = Box::new(Shared(self.link.clone()));
                        self.peer = Some(NetPeer::new(player, link, 0));
                        self.status = format!("player {player}: waiting for the host");
                    }
                    unbanished_net::FromRelay::Error { message } => {
                        self.status = format!("relay: {message}");
                        self.done = true;
                    }
                    _ => {}
                }
            }
            if let unbanished_net::State::Closed(e) = self.link.borrow().state() {
                self.status = format!("relay: {e}");
                self.done = true;
            }
            return !self.done;
        }
        use unbanished_game::behaviour::animation::shared;
        shared::swap(&mut self.table);
        let start = web_time::Instant::now();
        let (peer, target, lines) = (self.peer.as_mut().unwrap(), self.target, &mut self.lines);
        let outcome = game.with_ctx(|_, ctx| -> Result<bool, String> {
            while start.elapsed() < budget {
                peer.poll(ctx)?;
                let Some(g) = peer.game.as_ref() else {
                    return Ok(false);
                };
                if g.sim.updates > target {
                    return Ok(true);
                }
                let Some(update) = peer.begin(ctx) else {
                    return Ok(false);
                };
                peer.boundary(ctx, update)?;
                if update % 60 == 0 {
                    let d = peer.game.as_ref().unwrap().digest(ctx, update as u32)?;
                    lines.retain(|l: &String| !l.starts_with(&format!("update {update:>6} ")));
                    lines.push(d.to_string());
                }
                peer.update(ctx);
            }
            Ok(false)
        });
        shared::swap(&mut self.table);
        let player = self.peer.as_ref().map_or(0, |p| p.player);
        let at = self
            .peer
            .as_ref()
            .and_then(|p| p.game.as_ref())
            .map_or(0, |g| g.sim.updates);
        match outcome {
            Ok(true) => {
                self.done = true;
                self.status = format!("player {player}: done at update {at}");
            }
            Ok(false) => self.status = format!("player {player}: update {at} of {target}"),
            Err(e) => {
                self.done = true;
                self.status = format!("player {player}: {e}");
            }
        }
        !self.done
    }
}
