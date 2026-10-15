//! Rendering shared by the Banished apps (viewer and game): texture upload, the UI renderer and
//! model material classification.

pub mod material;
mod texture;
mod ui;

pub use texture::{
    decode_levels, upload_cube, upload_image, upload_image_linear, upload_rgba, upload_rgba_as,
};
pub use ui::UiRenderer;
