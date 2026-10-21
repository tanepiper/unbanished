//! The Multiplayer window: host a room on a relay or join one, wait for the players, and start
//! the game from the host's save. Not in the original, which has no multiplayer (see
//! docs/MULTIPLAYER-PLAN.md, step 6).
//!
//! Deviation: the window is new. It is built from the original's Map dialog
//! (`Dialog/MapParams.rsc`) with its rows given new labels, so it looks like the game's own
//! windows. The room code goes in the town name's edit and the relay's address in the seed's.
//! OK is Host while the code is empty and Join with a code; the host's becomes Start once a
//! player has joined, and Start asks for the save to play (the Load dialog).

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;

use unbanished_net::{FromRelay, State, ToRelay};
use unbanished_ui::{Control, Kind, NodeId, Ui};

use crate::gameplay::lockstep::Message;
use crate::gameplay::network::{Link, RelayLink, Shared};
use crate::gameplay::GameplayLoop;
use crate::loops::Screen;
use crate::settings::Persistence;

const DIALOG: &str = "Dialog/MapParams.rsc:resource";
/// The relay used when the settings name none.
pub const DEFAULT_RELAY: &str = "ws://127.0.0.1:7780";
/// Updates between a player's command and its slot (100 ms), and between the host's checks
/// (10 s).
const DELAY: u64 = 6;
const CHECK: u64 = 600;

/// The host's check interval: `UNBANISHED_NET_CHECK` (tests) or [`CHECK`].
fn check_interval() -> u64 {
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(n) = unbanished_formats::env::var("NET_CHECK")
        .ok()
        .and_then(|v| v.parse().ok())
    {
        return n;
    }
    CHECK
}

/// What a click or an update asks of the menu.
pub enum Outcome {
    Stay,
    Closed,
    /// The host chose Start: the menu opens the Load dialog for the save to play.
    ChooseSave,
    /// The game starts.
    Start(Box<GameplayLoop>),
}

enum Role {
    Idle,
    Hosting { players: BTreeSet<u8> },
    Joined { player: u8 },
}

pub struct MultiplayerDialog {
    pub root: NodeId,
    code: Option<NodeId>,
    relay: Option<NodeId>,
    players: Option<NodeId>,
    status: Option<NodeId>,
    ok: Option<NodeId>,
    cancel: Option<NodeId>,
    link: Option<Rc<RefCell<RelayLink>>>,
    /// Who this game is, for the relay.
    hello: unbanished_net::Hello,
    /// What arrived before the start (a running game's turns, for a player who joins it).
    early: Vec<(u8, Message)>,
    /// The host: the update of the room's kept town from the last game, if the server has one.
    kept: Option<u64>,
    /// The address of the server this game started for its room, for the others to enter.
    here: Option<String>,
    role: Role,
    message: String,
}

fn edit_text(ui: &Ui, edit: Option<NodeId>) -> String {
    match edit.map(|e| &ui.node(e).kind) {
        Some(Kind::Control(Control::Edit(e))) => e.text.text.trim().to_string(),
        _ => String::new(),
    }
}

impl MultiplayerDialog {
    pub fn open(
        ui: &mut Ui,
        screen: &mut Screen,
        settings: &Persistence,
        archive: &unbanished_formats::pkg::Archive,
    ) -> Option<Self> {
        let index = screen.add_modal(ui, DIALOG)?;
        let root = screen.dialogs[index].root;
        let find = |ui: &Ui, n: &str| ui.find(root, n);
        for (element, text) in [
            ("labelTitle", "Multiplayer"),
            ("descName", "Room Code"),
            ("descSeed", "Relay"),
            ("descTerrain", "Players"),
            ("descTerrainSize", "Status"),
        ] {
            if let Some(n) = find(ui, element) {
                ui.set_text(n, text);
            }
        }
        for element in [
            "buttonNewName",
            "descClimate",
            "textClimate",
            "descDisaster",
            "textDisaster",
            "descCondition",
            "textCondition",
        ] {
            if let Some(n) = find(ui, element) {
                ui.set_visible(n, false);
            }
        }
        let code = find(ui, "editName");
        if let Some(e) = code {
            ui.set_text(e, "");
            if let Kind::Control(Control::Edit(d)) = &mut ui.node_mut(e).kind {
                d.max_length = 12;
                // A town name takes no digits; a code is letters and digits.
                d.allowed = ('A'..='Z').chain('a'..='z').chain('0'..='9').collect();
            }
        }
        // The seed's edit takes digits only; the address is text.
        let relay = find(ui, "numberSeed");
        if let Some(e) = relay {
            if let Kind::Control(Control::Edit(d)) = &mut ui.node_mut(e).kind {
                d.numeric = false;
                d.range = None;
                // Printable ASCII with `:` (which edits otherwise refuse) for `ws://host:port`.
                d.allowed = (' '..='~').collect();
                d.max_length = 120;
            }
            let address = settings
                .get_text("Multiplayer", "Relay")
                .unwrap_or_else(|| DEFAULT_RELAY.to_string());
            ui.set_text(e, &address);
        }
        let d = MultiplayerDialog {
            hello: crate::gameplay::network::hello(archive),
            early: Vec::new(),
            kept: None,
            here: None,
            root,
            code,
            relay,
            players: find(ui, "textTerrain"),
            status: find(ui, "textTerrainSize"),
            ok: find(ui, "buttonOk"),
            cancel: find(ui, "buttonCancel"),
            link: None,
            role: Role::Idle,
            message: "Leave the code empty to host a room.".into(),
        };
        d.refresh(ui);
        Some(d)
    }

    /// The room code, the players and the status; OK's label and whether it can be pressed.
    fn refresh(&self, ui: &mut Ui) {
        let (players, ok, enabled) = match &self.role {
            Role::Idle => {
                let join = !edit_text(ui, self.code).is_empty();
                let label = if join { "Join" } else { "Host" };
                (String::new(), label, !edit_text(ui, self.relay).is_empty())
            }
            Role::Hosting { players, .. } => (list(players), "Start", players.len() > 1),
            Role::Joined { player, .. } => (format!("You are player {}", player + 1), "Ok", false),
        };
        if let Some(n) = self.players {
            ui.set_text(n, &players);
        }
        if let Some(n) = self.status {
            ui.set_text(n, &self.message);
        }
        if let Some(n) = self.ok {
            ui.set_text(n, ok);
            ui.node_mut(n).enabled = enabled;
        }
    }

    pub fn click(
        &mut self,
        ui: &mut Ui,
        screen: &mut Screen,
        settings: &mut Persistence,
        node: NodeId,
    ) -> Outcome {
        let is = |b: Option<NodeId>| b.is_some_and(|b| ui.descendant(b, node));
        if is(self.cancel) {
            self.close(ui, screen);
            return Outcome::Closed;
        }
        if is(self.ok) && ui.node(self.ok.unwrap()).enabled {
            match &self.role {
                Role::Idle => {
                    let relay = edit_text(ui, self.relay);
                    let code = edit_text(ui, self.code).to_uppercase();
                    settings.set_text("Multiplayer", "Relay", &relay);
                    settings.write();
                    log::info!("multiplayer: connecting to {relay}");
                    self.connect(&relay, code);
                }
                // A room with a kept town goes on from it; otherwise the host chooses a save.
                Role::Hosting { .. } if self.kept.is_some() => {
                    if let Some(link) = &self.link {
                        link.borrow_mut().request(ToRelay::Fetch);
                    }
                    self.message = "Fetching the room's town...".into();
                }
                Role::Hosting { .. } => {
                    log::info!("multiplayer: choosing the save to play");
                    return Outcome::ChooseSave;
                }
                Role::Joined { .. } => {}
            }
        }
        self.refresh(ui);
        Outcome::Stay
    }

    fn connect(&mut self, relay: &str, code: String) {
        // Hosting at this computer's own address, with no server running there: the game
        // starts one (desktop only) and says where the others reach it.
        #[cfg(not(target_arch = "wasm32"))]
        let here = match crate::platform::host_here::local_port(relay) {
            Some(port) if code.is_empty() => crate::platform::host_here::start(port),
            _ => None,
        };
        #[cfg(target_arch = "wasm32")]
        let here: Option<String> = None;
        let mut link = RelayLink::new(
            unbanished_net::Connection::connect(relay),
            self.hello.clone(),
        );
        if code.is_empty() {
            // `UNBANISHED_ROOM` asks the relay for that code (tests).
            #[cfg(not(target_arch = "wasm32"))]
            let room = unbanished_formats::env::var("ROOM").ok();
            #[cfg(target_arch = "wasm32")]
            let room = None;
            link.request(ToRelay::Host {
                room,
                password: None,
            });
            self.message = format!("Opening a room on {relay}...");
            self.here = here;
        } else {
            link.request(ToRelay::Join {
                room: code.clone(),
                password: None,
            });
            self.message = format!("Joining {code}...");
        }
        self.link = Some(Rc::new(RefCell::new(link)));
    }

    /// Each frame: what the relay and the host sent.
    pub fn update(&mut self, ui: &mut Ui, screen: &mut Screen) -> Outcome {
        let Some(link) = self.link.clone() else {
            self.refresh(ui);
            return Outcome::Stay;
        };
        let arrived = Link::poll(&mut *link.borrow_mut());
        let events: Vec<_> = link.borrow_mut().events.drain(..).collect();
        let mut kept_town = None;
        for e in events {
            match (e, &mut self.role) {
                // The room's kept town: the game goes on from it.
                (FromRelay::Kept { bytes, .. }, Role::Hosting { .. }) => kept_town = Some(bytes),
                (FromRelay::Hosted { room, kept, .. }, _) => {
                    self.kept = kept;
                    self.message = match kept {
                        Some(_) => format!(
                            "Room {room} is open, with its town from the last game: Start goes on from it."
                        ),
                        None => format!("Room {room} is open. Give the code to the players."),
                    };
                    if let Some(here) = &self.here {
                        self.message +=
                            &format!(" This computer is the server: players enter {here}.");
                    }
                    if let Some(c) = self.code {
                        ui.set_text(c, &room);
                    }
                    self.role = Role::Hosting {
                        players: BTreeSet::from([0]),
                    };
                }
                (FromRelay::Joined { room, player }, _) => {
                    self.message = format!("In room {room}. Waiting for the host to start.");
                    self.role = Role::Joined { player };
                }
                (FromRelay::PeerJoined { player }, Role::Hosting { players, .. }) => {
                    players.insert(player);
                }
                (FromRelay::PeerLeft { player }, Role::Hosting { players, .. }) => {
                    players.remove(&player);
                }
                (FromRelay::Error { message }, _) => {
                    self.message = format!("The relay: {message}");
                    self.link = None;
                    self.role = Role::Idle;
                }
                _ => {}
            }
        }
        if let Some(bytes) = kept_town {
            return self.host_start(ui, screen, bytes);
        }
        if let State::Closed(e) = link.borrow().state() {
            self.message = format!("No connection: {e}");
            self.link = None;
            self.role = Role::Idle;
        }
        let mut outcome = Outcome::Stay;
        if let Role::Joined { player, .. } = self.role {
            let mut arrived = arrived.into_iter();
            while let Some((from, message)) = arrived.next() {
                if matches!(message, Message::Start { .. }) {
                    // What came before the start (a running game's turns), and after it.
                    let mut early = std::mem::take(&mut self.early);
                    early.extend(arrived);
                    match self.game(player, message, early) {
                        Ok(g) => outcome = Outcome::Start(Box::new(g)),
                        Err(e) => self.message = format!("The host's save: {e}"),
                    }
                    break;
                }
                self.early.push((from, message));
            }
        }
        if let Outcome::Start(_) = outcome {
            self.close(ui, screen);
        } else {
            self.refresh(ui);
        }
        outcome
    }

    /// The host starts the game from the save `bytes` for everybody in the room.
    pub fn host_start(&mut self, ui: &mut Ui, screen: &mut Screen, bytes: Vec<u8>) -> Outcome {
        let Role::Hosting { players, .. } = &self.role else {
            return Outcome::Stay;
        };
        // The relay numbers players as they come; the lockstep counts them from 0 without gaps.
        let count = players.iter().max().map_or(1, |&p| p + 1);
        // Players who joined and left again before the start.
        let absent: Vec<u8> = (0..count).filter(|p| !players.contains(p)).collect();
        let Some(link) = self.link.clone() else {
            return Outcome::Stay;
        };
        let check = check_interval();
        // A server that keeps rooms keeps the town from its start.
        link.borrow_mut().keep(0, &bytes);
        let start = Message::Start {
            players: count,
            delay: DELAY,
            check,
            absent,
            bytes,
            update: 0,
            epoch: 0,
            speed: 0,
            paused: false,
            tools: Vec::new(),
        };
        link.borrow_mut().send(None, &start);
        match self.game(0, start, Vec::new()) {
            Ok(g) => {
                self.close(ui, screen);
                Outcome::Start(Box::new(g))
            }
            Err(e) => {
                self.message = format!("The save: {e}");
                self.refresh(ui);
                Outcome::Stay
            }
        }
    }

    fn game(
        &mut self,
        player: u8,
        start: Message,
        early: Vec<(u8, Message)>,
    ) -> Result<GameplayLoop, String> {
        let link = self.link.take().ok_or("no connection")?;
        let g = GameplayLoop::from_start(player, start, Box::new(Shared(link)), early)?;
        log::info!("multiplayer: player {} starts", player + 1);
        Ok(g)
    }

    pub fn close(&mut self, ui: &mut Ui, screen: &mut Screen) {
        screen.remove_root(ui, self.root);
    }
}

fn list(players: &BTreeSet<u8>) -> String {
    let names: Vec<String> = players.iter().map(|p| format!("{}", p + 1)).collect();
    format!("{} ({})", players.len(), names.join(", "))
}

/// Deviation: a Multiplayer button under Load (the original has no multiplayer). It is a copy of
/// the Load button with its own name and label.
pub fn add_menu_button(ui: &mut Ui, screen: &Screen, dialog: usize) {
    if crate::platform::mode::vanilla() {
        return;
    }
    let (Some(list), Some(load)) = (
        screen.find(ui, dialog, "buttonList"),
        screen.find(ui, dialog, "buttonLoad"),
    ) else {
        return;
    };
    let reference = ui.node(load).reference.clone();
    let Some(button) = ui.instantiate(&reference) else {
        log::warn!("menu: no copy of {reference}");
        return;
    };
    ui.node_mut(button).name = "buttonMultiplayer".into();
    if let Some(label) = ui.find(button, "labelLoadGame") {
        ui.node_mut(label).name = "labelMultiplayer".into();
        ui.set_text(label, "Multiplayer");
    }
    let children = &mut ui.node_mut(list).children;
    let at = children
        .iter()
        .position(|&c| c == load)
        .map_or(children.len(), |i| i + 1);
    children.insert(at, button);
}
