//! A 1:1 reimplementation of Banished's game logic.
//!
//! Every routine here is ported from the shipped binaries (see `docs/`), keeping the original's
//! arithmetic order and float precision so that results match the game bit for bit. Ported
//! functions name their source address; tests compare against data from real save files.

pub mod activities;
pub mod agents;
pub mod ai;
pub mod animation;
pub mod calendar;
pub mod character;
pub mod citizens;
pub mod construction;
pub mod environment;
pub mod events;
pub mod growth;
pub mod jobs;
pub mod life;
pub mod map;
pub mod math;
pub mod names;
pub mod needs;
pub mod noise;
pub mod pathfind;
pub mod picking;
pub mod placement;
pub mod produce;
pub mod professions;
pub mod random;
pub mod scheduler;
pub mod sectors;
pub mod seeding;
pub mod spawn;
pub mod status;
pub mod stock;
pub mod terrain;
pub mod town;
