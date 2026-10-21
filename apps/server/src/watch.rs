//! The server's own copy of each room's game (`simulate = DIR`): a thread per room runs it with
//! the operator's game data (`unbanished_session::watch::Watcher`), fed every message the players
//! send the whole room. Its digests are the reference in the room's checks; when the players'
//! games differ from it, it sends its save to the room.
//!
//! Only a room whose host has the server's game data and no mods is watched; other rooms are
//! checked by their players' digests alone.

use std::path::Path;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use unbanished_formats::pkg::{Archive, Package};
use unbanished_net::checks::SERVER;
use unbanished_net::{encode, FromRelay, Hello};
use unbanished_session::lockstep::Message;
use unbanished_session::watch::{Event, Watcher};

use crate::serve::Shared;

/// What a room's watcher is told.
pub enum In {
    /// A message a player sent the whole room.
    Message(Message),
    /// A player left the room.
    Left(u8),
    /// The room's check keeps the server's game: send its save.
    WantSave,
}

/// The server's game data, and its fingerprints (`Archive::fingerprints`), which a watched
/// room's host must have.
pub struct GameData {
    pub archive: Arc<Archive>,
    pub data: u32,
    pub mods: u32,
}

impl GameData {
    /// The game's packages in `dir` (`WinData`), as the game loads them.
    pub fn load(dir: &Path) -> Result<GameData, String> {
        let mut archive = Archive::default();
        for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
            let path = dir.join(name);
            let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            archive.add(Package::parse(bytes).map_err(|e| format!("{name}: {e}"))?);
        }
        let (data, mods) = archive.fingerprints();
        Ok(GameData {
            archive: Arc::new(archive),
            data,
            mods,
        })
    }

    /// Whether a room whose host said `hello` can be watched with this data (the same data, and
    /// no mods or rule hooks, which the server does not run).
    pub fn fits(&self, hello: &Hello) -> bool {
        hello.data == self.data && hello.mods == self.mods && hello.hooks == 0
    }
}

/// Starts watching `room`: a thread that runs its game. The way to tell it what happens.
pub fn start(room: String, archive: Arc<Archive>, shared: Arc<Mutex<Shared>>) -> Sender<In> {
    let (tx, rx) = channel();
    let name = format!("watch {room}");
    let spawned = std::thread::Builder::new()
        .name(name)
        .spawn(move || run(&room, Watcher::new(archive), rx, &shared));
    if let Err(e) = spawned {
        log::error!("watching a room: {e}");
    }
    tx
}

fn run(room: &str, mut watcher: Watcher, rx: Receiver<In>, shared: &Mutex<Shared>) {
    log::info!("room {room}: the server runs the game");
    loop {
        let first = match rx.recv_timeout(Duration::from_millis(5)) {
            Ok(m) => Some(m),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => break,
        };
        for m in first.into_iter().chain(rx.try_iter()) {
            let result = match m {
                In::Message(message) => watcher.receive(message),
                In::Left(player) => {
                    watcher.left(player);
                    Ok(())
                }
                In::WantSave => {
                    watcher.want_save();
                    Ok(())
                }
            };
            if let Err(e) = result {
                log::error!("room {room}: the server's game: {e}");
            }
        }
        let events = match watcher.advance() {
            Ok(events) => events,
            Err(e) => {
                log::error!("room {room}: the server's game: {e}");
                continue;
            }
        };
        for event in events {
            let mut s = shared.lock().unwrap();
            match event {
                Event::Digest { epoch, update, crc } => {
                    let out = s.relay.server_digest(room, epoch, update, crc);
                    s.deliver(out);
                    if s.relay.take_server_saves().iter().any(|r| r == room) {
                        watcher.want_save();
                    }
                }
                Event::Save { update, bytes } => {
                    log::info!(
                        "room {room}: the players go on from the server's game at update {update}"
                    );
                    let data = encode(&Message::Save { update, bytes });
                    let out = s
                        .relay
                        .members(room)
                        .into_iter()
                        .map(|c| {
                            let data = data.clone();
                            (c, FromRelay::Data { from: SERVER, data })
                        })
                        .collect();
                    s.deliver(out);
                }
            }
        }
    }
    log::info!("room {room}: the server stops running the game");
}

/// A player sent the whole room `data`: the room's game starts being watched at its start, and
/// a watched room's game hears it.
pub fn heard(s: &mut Shared, shared: &Arc<Mutex<Shared>>, room: &str, data: &[u8]) {
    let Some(game) = &s.game else { return };
    let Ok(message) = unbanished_net::decode::<Message>(data) else {
        return;
    };
    if matches!(message, Message::Start { update: 0, .. }) && !s.watchers.contains_key(room) {
        if s.relay.host_hello(room).is_some_and(|h| game.fits(h)) {
            let tx = start(room.to_string(), game.archive.clone(), shared.clone());
            s.watchers.insert(room.to_string(), tx);
            s.relay.watch(room);
        } else {
            log::info!(
                "room {room}: other game data, mods or rule hooks; the players check each other"
            );
        }
    }
    if let Some(tx) = s.watchers.get(room) {
        let _ = tx.send(In::Message(message));
    }
}

/// A player left a room: its game stops waiting for that player, and an empty room's game
/// stops.
pub fn left(s: &mut Shared, room: &str, player: u8) {
    if s.relay.members(room).is_empty() {
        s.watchers.remove(room);
    } else if let Some(tx) = s.watchers.get(room) {
        let _ = tx.send(In::Left(player));
    }
}

/// The rooms whose check keeps the server's own game: their watchers send its save.
pub fn saves_wanted(s: &mut Shared) {
    for room in s.relay.take_server_saves() {
        if let Some(tx) = s.watchers.get(&room) {
            let _ = tx.send(In::WantSave);
        }
    }
}
