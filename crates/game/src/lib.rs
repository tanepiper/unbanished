#![doc = include_str!("../README.md")]
#![doc(
    html_logo_url = "https://tanepiper.github.io/unbanished/assets/un-banished-icon.svg",
    html_favicon_url = "https://tanepiper.github.io/unbanished/favicon.svg"
)]

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
