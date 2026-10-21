//! The core of Un-Banished: a Banished 1.0.7 town as the simulation sees it, with no drawing,
//! sound or interface. The town (`town`), its map (`town::map`), the save it is read from and
//! written to (`saved`), and the mod lists that choose its templates (`mods`).
//!
//! The game app draws and plays this town; tests and a server can run it with no window.

pub mod achievements;
pub mod command;
pub mod mods;
pub mod saved;
pub mod simulation;
pub mod strings;
pub mod timing;
pub mod tools;
pub mod town;
pub mod transform;
