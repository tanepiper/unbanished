//! The relay's rooms, as a state machine with no sockets or threads: a request from a game goes
//! in, and the messages for each game come out. The relay program (`apps/server`), the in-memory
//! transport ([`crate::memory`]) and tests use the same rules.

use std::collections::BTreeMap;

use crate::checks::{Checks, Verdict, SERVER};
use crate::protocol::{FromRelay, Hello, RoomInfo, ToRelay, PROTOCOL};
use crate::store::{keepable, Kept, Store};

/// A game connected to the relay, by a number the caller gives it.
pub type Client = u64;

/// What one request did: the messages to send, to whom, in order, and whether the relay should
/// close this game's connection (after sending them).
#[derive(Debug, Default, PartialEq)]
pub struct Reply {
    /// The messages, each for one game.
    pub out: Vec<(Client, FromRelay)>,
    /// Close the connection of the game that asked.
    pub close: bool,
}

struct Room {
    password: Option<String>,
    /// The host's `Hello`: every player must match it.
    host: Hello,
    /// The players by number; `None` once they left.
    players: Vec<Option<Client>>,
    /// The players' digests.
    checks: Checks,
    /// The server runs this room's game: its digests are the reference.
    watched: bool,
}

impl Room {
    /// The numbers of the players in the room, and the server's when it runs the game.
    fn present(&self) -> Vec<u8> {
        let numbered = self.players.iter().enumerate();
        let mut present: Vec<u8> = numbered
            .filter(|(_, c)| c.is_some())
            .map(|(p, _)| p as u8)
            .collect();
        // The server's own game, when it runs one, is checked with the players'.
        if self.watched {
            present.push(SERVER);
        }
        present
    }

    /// A check's verdict, told to every player; when the players differ, the one with the game
    /// to keep is also asked for its save. The messages, and whether the server's own game is the one to keep (its save is wanted).
    fn judged(
        &self,
        room: &str,
        update: u64,
        verdict: Verdict,
    ) -> (Vec<(Client, FromRelay)>, bool) {
        let agree = verdict == Verdict::Agree;
        let mut out: Vec<(Client, FromRelay)> = self
            .players
            .iter()
            .flatten()
            .map(|&c| (c, FromRelay::Checked { update, agree }))
            .collect();
        match verdict {
            Verdict::Agree => log::info!("room {room}: the players agree at update {update}"),
            Verdict::Differ { from } => {
                log::warn!(
                    "room {room}: the players differ at update {update}; player {from}'s game is kept"
                );
                if from == SERVER {
                    return (out, true);
                }
                if let Some(c) = self.players.get(from as usize).copied().flatten() {
                    out.push((c, FromRelay::SaveWanted { update }));
                }
            }
        }
        (out, false)
    }
}

#[derive(Default)]
struct Game {
    hello: Option<Hello>,
    room: Option<(String, u8)>,
}

/// The rooms and the games connected to a relay.
pub struct Relay {
    rooms: BTreeMap<String, Room>,
    games: BTreeMap<Client, Game>,
    /// The state of the room-code generator.
    seed: u64,
    /// The most rooms open at once; a host beyond it is refused.
    pub max_rooms: usize,
    /// Where rooms' towns are kept, if the server keeps them.
    store: Option<Box<dyn Store>>,
    /// Rooms whose server-run game is the one to keep: the server sends their saves.
    server_saves: Vec<String>,
}

impl Relay {
    /// An empty relay; `seed` starts its room codes.
    pub fn new(seed: u64) -> Relay {
        Relay {
            rooms: BTreeMap::new(),
            games: BTreeMap::new(),
            seed: seed | 1,
            max_rooms: usize::MAX,
            store: None,
            server_saves: Vec::new(),
        }
    }

    /// Keeps the rooms' towns in `store`.
    pub fn with_store(mut self, store: Box<dyn Store>) -> Relay {
        self.store = Some(store);
        self
    }

    /// How many rooms are open.
    pub fn room_count(&self) -> usize {
        self.rooms.len()
    }

    /// How many games are connected.
    pub fn game_count(&self) -> usize {
        self.games.len()
    }

    /// The room and player number of a connected game, if it is in a room.
    pub fn room_of(&self, client: Client) -> Option<(String, u8)> {
        self.games.get(&client)?.room.clone()
    }

    /// The `Hello` of a room's host.
    pub fn host_hello(&self, room: &str) -> Option<&Hello> {
        self.rooms.get(room).map(|r| &r.host)
    }

    /// The games in a room.
    pub fn members(&self, room: &str) -> Vec<Client> {
        self.rooms
            .get(room)
            .map(|r| r.players.iter().flatten().copied().collect())
            .unwrap_or_default()
    }

    /// The server runs this room's game: its digests are the reference from now on.
    pub fn watch(&mut self, room: &str) {
        if let Some(r) = self.rooms.get_mut(room) {
            r.watched = true;
        }
    }

    /// The server's own digest of a room's game (see [`Relay::watch`]): the messages the
    /// check gives.
    pub fn server_digest(
        &mut self,
        room: &str,
        epoch: u32,
        update: u64,
        crc: u32,
    ) -> Vec<(Client, FromRelay)> {
        let Some(r) = self.rooms.get_mut(room) else {
            return Vec::new();
        };
        let present = r.present();
        log::debug!(
            "room {room}: the server's digest of update {update} (epoch {epoch}): {crc:08x}"
        );
        let Some((update, verdict)) = r.checks.digest(SERVER, epoch, update, crc, &present) else {
            return Vec::new();
        };
        let (out, server) = r.judged(room, update, verdict);
        if server {
            self.server_saves.push(room.to_string());
        }
        out
    }

    /// The rooms whose server-run game is to be kept since the last call: the server sends each
    /// its save.
    pub fn take_server_saves(&mut self) -> Vec<String> {
        std::mem::take(&mut self.server_saves)
    }

    /// A game connected.
    pub fn connect(&mut self, client: Client) {
        self.games.insert(client, Game::default());
    }

    /// A game's request.
    pub fn handle(&mut self, client: Client, request: ToRelay) -> Reply {
        let mut reply = Reply::default();
        let refuse = |reply: &mut Reply, message: String| {
            reply.out.push((client, FromRelay::Error { message }));
        };
        let Some(game) = self.games.get_mut(&client) else {
            return reply;
        };
        if game.hello.is_none() {
            match request {
                ToRelay::Hello(hello) if hello.protocol == PROTOCOL => {
                    game.hello = Some(hello);
                    reply
                        .out
                        .push((client, FromRelay::Welcome { protocol: PROTOCOL }));
                }
                ToRelay::Hello(hello) => {
                    let message = format!(
                        "this relay speaks protocol {PROTOCOL} and the game speaks {}: use a \
                         relay of the same Un-Banished version",
                        hello.protocol
                    );
                    refuse(&mut reply, message);
                    reply.close = true;
                }
                _ => {
                    refuse(&mut reply, "a game says Hello first".into());
                    reply.close = true;
                }
            }
            return reply;
        }
        match request {
            ToRelay::Hello(_) => refuse(&mut reply, "Hello was already said".into()),
            ToRelay::Ping(n) => reply.out.push((client, FromRelay::Pong(n))),
            ToRelay::ListRooms => {
                let rooms = self
                    .rooms
                    .iter()
                    .map(|(code, r)| RoomInfo {
                        code: code.clone(),
                        players: r.players.iter().flatten().count() as u8,
                        password: r.password.is_some(),
                    })
                    .collect();
                reply.out.push((client, FromRelay::Rooms(rooms)));
            }
            ToRelay::Host { room, password } => {
                if game.room.is_some() {
                    refuse(&mut reply, "this game is in a room: leave it first".into());
                    return reply;
                }
                let hello = game.hello.clone().expect("said Hello");
                if self.rooms.len() >= self.max_rooms {
                    refuse(&mut reply, "the server is full: try again later".into());
                    return reply;
                }
                let room = room.unwrap_or_else(|| self.new_code());
                if self.rooms.contains_key(&room) {
                    refuse(&mut reply, format!("room {room} is in use"));
                    return reply;
                }
                self.rooms.insert(
                    room.clone(),
                    Room {
                        password,
                        host: hello,
                        players: vec![Some(client)],
                        checks: Checks::default(),
                        watched: false,
                    },
                );
                log::info!("room {room} opened");
                let keeps = self.store.is_some() && keepable(&room);
                let kept = self
                    .store
                    .as_ref()
                    .filter(|_| keeps)
                    .and_then(|s| s.load(&room))
                    .map(|k| k.update);
                reply.out.push((
                    client,
                    FromRelay::Hosted {
                        room: room.clone(),
                        keeps,
                        kept,
                    },
                ));
                self.games.get_mut(&client).expect("connected").room = Some((room, 0));
            }
            ToRelay::Join { room, password } => {
                if game.room.is_some() {
                    refuse(&mut reply, "this game is in a room: leave it first".into());
                    return reply;
                }
                let hello = game.hello.clone().expect("said Hello");
                let Some(r) = self.rooms.get_mut(&room) else {
                    refuse(&mut reply, format!("no room {room}"));
                    return reply;
                };
                if r.password.is_some() && r.password != password {
                    refuse(
                        &mut reply,
                        format!("the password for room {room} is not right"),
                    );
                    return reply;
                }
                if let Some(why) = r.host.differs(&hello) {
                    refuse(&mut reply, why);
                    return reply;
                }
                let player = r.players.len() as u8;
                for &p in r.players.iter().flatten() {
                    reply.out.push((p, FromRelay::PeerJoined { player }));
                }
                r.players.push(Some(client));
                log::info!("room {room}: player {player} joined");
                reply.out.push((
                    client,
                    FromRelay::Joined {
                        room: room.clone(),
                        player,
                    },
                ));
                self.games.get_mut(&client).expect("connected").room = Some((room, player));
            }
            ToRelay::Leave => reply.out = self.leave(client),
            // Only a room's host keeps its town, and only on a server that keeps rooms.
            ToRelay::Keep { update, bytes } => {
                if let (Some((room, 0)), Some(store)) = (game.room.clone(), self.store.as_mut()) {
                    store.keep(&room, Kept { update, bytes });
                    log::info!("room {room}: kept the town at update {update}");
                }
            }
            ToRelay::Fetch => {
                let kept = match (&game.room, &self.store) {
                    (Some((room, _)), Some(store)) => store.load(room),
                    _ => None,
                };
                let message = match kept {
                    Some(Kept { update, bytes }) => FromRelay::Kept { update, bytes },
                    None => FromRelay::Error {
                        message: "this room has no kept town".into(),
                    },
                };
                reply.out.push((client, message));
            }
            ToRelay::Digest { epoch, update, crc } => {
                let Some((room, player)) = game.room.clone() else {
                    return reply;
                };
                let Some(r) = self.rooms.get_mut(&room) else {
                    return reply;
                };
                let present = r.present();
                if let Some((update, verdict)) =
                    r.checks.digest(player, epoch, update, crc, &present)
                {
                    let (out, server) = r.judged(&room, update, verdict);
                    reply.out = out;
                    if server {
                        self.server_saves.push(room);
                    }
                }
            }
            ToRelay::Send { to, data } => {
                let Some((room, from)) = game.room.clone() else {
                    return reply;
                };
                let Some(r) = self.rooms.get(&room) else {
                    return reply;
                };
                for (p, c) in r.players.iter().enumerate() {
                    let wanted = match to {
                        Some(t) => p == t as usize,
                        None => p != from as usize,
                    };
                    if let (true, Some(c)) = (wanted, c) {
                        let data = data.clone();
                        reply.out.push((*c, FromRelay::Data { from, data }));
                    }
                }
            }
        }
        reply
    }

    /// A game's connection closed: it leaves its room.
    pub fn disconnect(&mut self, client: Client) -> Vec<(Client, FromRelay)> {
        let out = self.leave(client);
        self.games.remove(&client);
        out
    }

    /// The game leaves its room: the others hear of it, and an empty room closes.
    fn leave(&mut self, client: Client) -> Vec<(Client, FromRelay)> {
        let mut out = Vec::new();
        let Some((room, player)) = self.games.get_mut(&client).and_then(|g| g.room.take()) else {
            return out;
        };
        if let Some(r) = self.rooms.get_mut(&room) {
            r.players[player as usize] = None;
            for &p in r.players.iter().flatten() {
                out.push((p, FromRelay::PeerLeft { player }));
            }
            // The checks no longer wait for the player who left.
            let present = r.present();
            for (update, verdict) in r.checks.left(&present) {
                let (messages, server) = r.judged(&room, update, verdict);
                out.extend(messages);
                if server {
                    self.server_saves.push(room.clone());
                }
            }
            if r.players.iter().all(Option::is_none) {
                self.rooms.remove(&room);
                log::info!("room {room} closed");
            }
        }
        out
    }

    /// A new room code: six letters, not in use.
    fn new_code(&mut self) -> String {
        loop {
            let code: String = (0..6)
                .map(|_| {
                    self.seed ^= self.seed << 13;
                    self.seed ^= self.seed >> 7;
                    self.seed ^= self.seed << 17;
                    // Letters that cannot be mistaken for each other or for digits.
                    b"ABCDEFGHJKLMNPQRSTUVWXYZ"[(self.seed % 24) as usize] as char
                })
                .collect();
            if !self.rooms.contains_key(&code) {
                return code;
            }
        }
    }
}
