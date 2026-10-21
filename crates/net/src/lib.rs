//! Un-Banished multiplayer networking: the protocol between a game and the relay, and a WebSocket
//! connection to the relay that works natively and in the browser.
//!
//! The relay (`apps/relay`) keeps rooms. A game says [`Hello`] (the protocol version, its engine,
//! fingerprints of its game data and mods), then hosts a room and gets its code, or joins one
//! with the code. The relay numbers the players in the order they come (the host is 0), refuses
//! a game that differs from the host with the reason, and passes their data on, to one player or
//! to all the others. It never reads the data: the game's own messages (its lockstep turns,
//! digests and saves) travel inside [`ToRelay::Send`] and [`FromRelay::Data`]. Every WebSocket
//! message is one value, encoded with postcard. The rules are in [`rooms`]; [`memory`] runs a
//! relay in one process for tests.
//!
//! Not in the original game, which has no multiplayer; see docs/MULTIPLAYER-PLAN.md, step 5.
#![doc(
    html_logo_url = "https://tanepiper.github.io/unbanished/assets/un-banished-icon.svg",
    html_favicon_url = "https://tanepiper.github.io/unbanished/favicon.svg"
)]

pub mod memory;
mod protocol;
pub mod rooms;

pub use protocol::{decode, encode, FromRelay, Hello, RoomInfo, ToRelay, PROTOCOL};

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
mod tests;
