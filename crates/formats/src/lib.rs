//! Readers for Shining Rock engine data (Banished).
//!
//! Format notes and the addresses they were recovered from are in `docs/FINDINGS.md`.
//! Nothing here embeds game data; callers supply bytes from their own install.

pub mod audio;
pub mod crc;
pub mod crs;
pub mod error;
pub mod game;
pub mod lzf;
pub mod menu_scene;
pub mod names;
pub mod par;
pub mod picking;
pub mod pkg;
pub mod registry;
pub mod save;
pub mod savegame;
pub mod savegame_schema;
pub mod scenario;
pub mod skeleton;
pub mod stream;
pub mod texture;
pub mod types;
pub mod ui;
pub mod weather;

pub use error::{Error, Result};
