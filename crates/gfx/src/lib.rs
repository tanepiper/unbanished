//! Rendering shared by the Banished apps (viewer and game): texture upload and the UI renderer.

mod texture;
mod ui;

pub use texture::{upload_image, upload_rgba, upload_rgba_as};
pub use ui::UiRenderer;
