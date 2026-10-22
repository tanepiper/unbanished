//! The players' games over a network: a [`Link`] to the other players, and a [`NetPeer`] that
//! drives one player's lockstep game over it (turns, the host's digest checks and resyncs, the
//! start from the host's save). The link is the relay ([`RelayLink`], `unbanished-net`), or a
//! simulated network in one process for tests ([`simulate`]). See docs/MULTIPLAYER-PLAN.md,
//! steps 3 to 5.
//!
//! Deviation: not in the original, which has no multiplayer.
// The browser joins through the same code once a page asks for it; until then parts are unused.
#![cfg_attr(target_arch = "wasm32", allow(dead_code))]

mod peer;
mod simulate;
mod turns;
pub use peer::NetPeer;
#[cfg_attr(target_arch = "wasm32", allow(unused_imports))]
pub use simulate::{simulate, Script, Simulation};

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
            let events = std::mem::take(&mut self.link.borrow_mut().events);
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
