#![doc = include_str!("../README.md")]
#![doc(
    html_logo_url = "https://tanepiper.github.io/unbanished/assets/un-banished-icon.svg",
    html_favicon_url = "https://tanepiper.github.io/unbanished/favicon.svg"
)]

/// The bit-exact foundations, from their own crate (`unbanished-core`).
pub use unbanished_core::{calendar, crt, math, noise, picking, random, scheduler};
/// Path finding (`unbanished-path`) and the terrain (`unbanished-terrain`), from their own crates.
pub use unbanished_path::{pathfind, sectors};
pub use unbanished_terrain as terrain;

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
