//! What the game shows of a town: the scene and its assets, the world view, particles, weather and the map parameters.

#[cfg(all(feature = "cinematic", not(target_arch = "wasm32")))]
pub mod cinematic;
pub mod map_params;
pub mod particles;
pub mod scene;
pub mod scene_assets;
pub mod town;
pub mod weather;
pub mod world;
