//! The player's tools: placing buildings and areas, removing, roads and paths, selecting, and following a citizen.

pub mod follow;
pub use unbanished_sim::tools::{path_tool, placement_tool, removal_tool};
#[cfg(test)]
mod placement_tests;
#[cfg(test)]
mod removal_tests;
pub mod selection;
