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

pub mod activities;
pub mod agents;
pub mod ai;
pub mod animation;
pub mod blight;
pub mod boat;
pub mod cemetery;
pub mod character;
pub mod character_tree;
pub mod citizens;
pub mod commute;
pub mod construction;
pub mod control;
pub mod destroy;
pub mod environment;
pub mod events;
pub mod farm;
pub mod fire;
pub mod fishing;
pub mod floor;
pub mod forester;
pub mod growth;
pub mod happiness;
pub mod herbalist;
pub mod hunt;
pub mod immigration;
pub mod jobs;
pub mod life;
pub mod map;
pub mod market;
pub mod names;
pub mod needs;
pub mod orchard;
pub mod pasture;
pub mod placement;
pub mod produce;
pub mod professions;
pub mod quarry;
pub mod road;
pub mod seeding;
pub mod services;
pub mod spawn;
pub mod status;
pub mod stock;
pub mod terraform;
pub mod tornado;
pub mod town;
pub mod tracker;
pub mod trade;
pub mod wildlife;
