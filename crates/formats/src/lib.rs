//! Readers for Shining Rock engine data (Banished).
//!
//! Format notes and the addresses they were recovered from are in `docs/FINDINGS.md`.
//! Nothing here embeds game data; callers supply bytes from their own install.

pub mod crc;
pub mod crs;
pub mod error;
pub mod lzf;
pub mod pkg;
pub mod skeleton;
pub mod stream;
pub mod texture;
pub mod types;
pub mod ui;

pub use error::{Error, Result};
