#![doc = include_str!("../README.md")]
#![doc(
    html_logo_url = "https://tanepiper.github.io/unbanished/assets/un-banished-icon.svg",
    html_favicon_url = "https://tanepiper.github.io/unbanished/favicon.svg"
)]

pub mod audio;
pub mod crc;
pub mod crs;
/// The error type of every reader.
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
