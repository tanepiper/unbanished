//! A 1:1 reimplementation of Banished's game logic.
//!
//! Every routine here is ported from the shipped binaries (see `docs/`), keeping the original's
//! arithmetic order and float precision so that results match the game bit for bit. Ported
//! functions name their source address; tests compare against data from real save files.

/// The bit-exact foundations, from their own crate (`banished-core`).
pub use banished_core::{calendar, crt, math, noise, picking, random, scheduler};
/// Path finding (`banished-path`) and the terrain (`banished-terrain`), from their own crates.
pub use banished_path::{pathfind, sectors};
pub use banished_terrain as terrain;

pub mod behaviour;
pub mod commerce;
pub mod disasters;
pub mod events;
pub mod nature;
pub mod people;
pub mod production;
pub mod town;
pub mod work;
pub mod world;
