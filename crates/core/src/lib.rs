//! The bit-exact foundations the rest of Un-Banished builds on. Nothing here depends on the game's
//! rules, data files or other crates:
//!
//! - [`crt`]: the MSVC runtime's `sinf`, `cosf`, `acosf`, `atan2f` and friends, ported so float
//!   results match the original game to the last bit;
//! - [`random`]: the game's Mersenne Twister (`System::Random`) and its draws;
//! - [`noise`]: the Perlin noise the terrain generator uses;
//! - [`math`]: vectors, quaternions and the engine's `Transform`;
//! - [`scheduler`]: game time (`System::Time`, integer microseconds) and the entity scheduler's
//!   buckets;
//! - [`calendar`]: months, seasons and years from game time;
//! - [`picking`]: the engine's ray–triangle picking (`Picking::Scene`).
//!
//! `banished-game` re-exports these modules, so `banished_game::random` and
//! `banished_core::random` are the same module.

pub mod calendar;
pub mod crt;
pub mod math;
pub mod noise;
pub mod picking;
pub mod random;
pub mod scheduler;
