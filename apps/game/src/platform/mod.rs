//! The platform around the game: file locations, the browser glue, the engine version and the frame profiler.

#[cfg(feature = "mod-sites")]
pub mod nexus;
pub mod paths;
pub mod perf;
pub mod version;
#[cfg(target_arch = "wasm32")]
pub mod web;
