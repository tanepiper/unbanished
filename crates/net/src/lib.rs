//! Un-Banished multiplayer networking: the protocol between a game and the relay, and a WebSocket
//! connection to the relay that works natively and in the browser.
//!
//! The relay (`apps/relay`) keeps rooms. A host opens one and gets its code; players join with
//! the code. The relay numbers the players in the order they come (the host is 0) and passes
//! their data on, to one player or to all the others. It never reads the data: the game's own
//! messages (its lockstep turns, digests and saves) travel inside [`ToRelay::Send`] and
//! [`FromRelay::Data`]. Every WebSocket message is one value, encoded with postcard.
//!
//! Not in the original game, which has no multiplayer; see docs/MULTIPLAYER-PLAN.md, step 5.
#![doc(
    html_logo_url = "https://tanepiper.github.io/unbanished/assets/un-banished-icon.svg",
    html_favicon_url = "https://tanepiper.github.io/unbanished/favicon.svg"
)]

use serde::{Deserialize, Serialize};

/// A game to the relay.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ToRelay {
    /// Open a room (the code it asks for, or a new one) and be its host, player 0.
    Host {
        /// The code to use, or none for a new one.
        room: Option<String>,
    },
    /// Join a room.
    Join {
        /// The room's code.
        room: String,
    },
    /// Data for one player, or for every other player in the room.
    Send {
        /// The player it is for, or none for every other player.
        to: Option<u8>,
        /// The data; the relay does not read it.
        data: Vec<u8>,
    },
}

/// The relay to a game.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FromRelay {
    /// The room is open: its code (this game is player 0).
    Hosted {
        /// The room's code.
        room: String,
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
    /// A request failed (no such room, ...).
    Error {
        /// What went wrong.
        message: String,
    },
}

/// Encodes a protocol value (or a game message) for a WebSocket binary message.
pub fn encode<T: Serialize>(value: &T) -> Vec<u8> {
    postcard::to_stdvec(value).expect("a protocol value encodes")
}

/// Decodes a WebSocket binary message.
pub fn decode<'a, T: Deserialize<'a>>(bytes: &'a [u8]) -> Result<T, String> {
    postcard::from_bytes(bytes).map_err(|e| e.to_string())
}

/// The state of a connection.
#[derive(Debug, Clone, PartialEq)]
pub enum State {
    /// The connection is being made.
    Connecting,
    /// The relay has answered.
    Open,
    /// The connection is closed, for this reason.
    Closed(String),
}

#[cfg(feature = "native")]
mod native;
#[cfg(feature = "native")]
pub use native::Connection;

#[cfg(target_arch = "wasm32")]
mod web;
#[cfg(target_arch = "wasm32")]
pub use web::Connection;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_round_trip() {
        let values = [
            ToRelay::Host { room: None },
            ToRelay::Join {
                room: "ABCDEF".into(),
            },
            ToRelay::Send {
                to: Some(2),
                data: vec![1, 2, 3],
            },
        ];
        for v in values {
            assert_eq!(decode::<ToRelay>(&encode(&v)).unwrap(), v);
        }
        let big = FromRelay::Data {
            from: 1,
            data: vec![7; 1 << 20],
        };
        let bytes = encode(&big);
        assert!(bytes.len() < (1 << 20) + 16, "{}", bytes.len());
        assert_eq!(decode::<FromRelay>(&bytes).unwrap(), big);
    }
}
