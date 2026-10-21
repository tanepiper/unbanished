//! The relay protocol, version [`PROTOCOL`]: what a game and the relay say to each other.
//!
//! A game's first message is [`ToRelay::Hello`]. The relay answers [`FromRelay::Welcome`] when
//! it speaks the same protocol, and [`FromRelay::Error`] with the reason when it does not. Then
//! the lobby: list the rooms, host one (with a password if wanted), join one, leave, ping. The
//! players of one room must run the same engine, the same game data and the same mods; the relay
//! compares each joining game's `Hello` with the host's and says what differs. Inside a room the
//! game's own messages (lockstep turns, saves) travel as [`ToRelay::Send`] and
//! [`FromRelay::Data`], which the relay never reads. The digests of the players' games go to the
//! server itself ([`ToRelay::Digest`]): it compares them (`crate::checks`) and, when the players
//! differ, asks one whose game most players share for its save ([`FromRelay::SaveWanted`]).
//!
//! Before Un-Banished 1.0 a new protocol version replaces the old one; there is no
//! compatibility between versions.

use serde::{Deserialize, Serialize};

/// The protocol version this crate speaks.
pub const PROTOCOL: u16 = 3;

/// Who a game is: the protocol it speaks, and what its players must share.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hello {
    /// The protocol version ([`PROTOCOL`]).
    pub protocol: u16,
    /// The engine's name and version, for example `Un-Banished 0.4.0-707cfea`.
    pub engine: String,
    /// A fingerprint of the game data (the original's packages).
    pub data: u32,
    /// A fingerprint of the enabled mods, in their order.
    pub mods: u32,
}

impl Hello {
    /// This game's `Hello` with the current protocol.
    pub fn new(engine: &str, data: u32, mods: u32) -> Hello {
        Hello {
            protocol: PROTOCOL,
            engine: engine.to_string(),
            data,
            mods,
        }
    }

    /// Why a game saying `other` cannot play in a room with this one, if it cannot.
    pub fn differs(&self, other: &Hello) -> Option<String> {
        if self.engine != other.engine {
            return Some(format!(
                "the host runs {}, and this game runs {}: use the same version",
                self.engine, other.engine
            ));
        }
        if self.data != other.data {
            return Some("this game's data differs from the host's".into());
        }
        if self.mods != other.mods {
            return Some("this game's mods differ from the host's: enable the same ones".into());
        }
        None
    }
}

/// A room in the relay's list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomInfo {
    /// The room's code.
    pub code: String,
    /// How many players are in it.
    pub players: u8,
    /// Whether it asks for a password.
    pub password: bool,
}

/// A game to the relay.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ToRelay {
    /// The first message: who this game is.
    Hello(Hello),
    /// The rooms open on this relay.
    ListRooms,
    /// Open a room (the code it asks for, or a new one) and be its host, player 0.
    Host {
        /// The code to use, or none for a new one.
        room: Option<String>,
        /// A password the players must give, or none.
        password: Option<String>,
    },
    /// Join a room.
    Join {
        /// The room's code.
        room: String,
        /// The room's password, if it has one.
        password: Option<String>,
    },
    /// Leave the room (the connection stays open).
    Leave,
    /// Data for one player, or for every other player in the room.
    Send {
        /// The player it is for, or none for every other player.
        to: Option<u8>,
        /// The data; the relay does not read it.
        data: Vec<u8>,
    },
    /// Is the relay there? It answers [`FromRelay::Pong`] with the same number.
    Ping(u32),
    /// The host's town, for the server to keep (when it keeps rooms): the save made when
    /// `update` updates had begun.
    Keep {
        /// The update the save was made at.
        update: u64,
        /// The save file's bytes.
        bytes: Vec<u8>,
    },
    /// The town the server keeps for this room: it answers [`FromRelay::Kept`].
    Fetch,
    /// A digest of this game's state when `update` updates had begun, made in `epoch` (the
    /// resyncs it has been through), for the server's check.
    Digest {
        /// The resyncs this game has been through.
        epoch: u32,
        /// The update.
        update: u64,
        /// The CRC-32 of the game state.
        crc: u32,
    },
}

/// The relay to a game.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FromRelay {
    /// The relay speaks this game's protocol.
    Welcome {
        /// The relay's protocol version.
        protocol: u16,
    },
    /// The rooms open on this relay.
    Rooms(Vec<RoomInfo>),
    /// The room is open: its code (this game is player 0).
    Hosted {
        /// The room's code.
        room: String,
        /// Whether the server keeps rooms' towns: the host sends it its saves ([`ToRelay::Keep`]).
        keeps: bool,
        /// The update of the town kept for this room from an earlier game, if there is one.
        kept: Option<u64>,
    },
    /// The town kept for this room.
    Kept {
        /// The update the save was made at.
        update: u64,
        /// The save file's bytes.
        bytes: Vec<u8>,
    },
    /// This game joined the room as `player`.
    Joined {
        /// The room's code.
        room: String,
        /// This game's player number.
        player: u8,
    },
    /// Another player came into the room.
    PeerJoined {
        /// Its player number.
        player: u8,
    },
    /// Another player left the room.
    PeerLeft {
        /// Its player number.
        player: u8,
    },
    /// Data from a player.
    Data {
        /// The sender's player number.
        from: u8,
        /// The data.
        data: Vec<u8>,
    },
    /// The answer to [`ToRelay::Ping`].
    Pong(u32),
    /// The server's check of `update`, to every player in the room: the games agree, or not.
    Checked {
        /// The update checked.
        update: u64,
        /// Whether every player's game was the same.
        agree: bool,
    },
    /// The players' games differed at `update`, and this game is the one to keep: send the
    /// others a save at the next update boundary.
    SaveWanted {
        /// The update where the games differed.
        update: u64,
    },
    /// A request failed, with the reason (no such room, a wrong password, another version...).
    Error {
        /// What went wrong, for the player to read.
        message: String,
    },
}

/// Encodes a protocol value (or a game message) for a WebSocket binary message.
pub fn encode<T: Serialize>(value: &T) -> Vec<u8> {
    postcard::to_stdvec(value).expect("a protocol value encodes")
}

/// Decodes a WebSocket binary message. A cut-off or unknown message is an error, never a panic.
pub fn decode<'a, T: Deserialize<'a>>(bytes: &'a [u8]) -> Result<T, String> {
    postcard::from_bytes(bytes).map_err(|e| e.to_string())
}
