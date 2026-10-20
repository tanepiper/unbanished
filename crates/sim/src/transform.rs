//! A placed object's transform, as the game draws and picks it.

use glam::{Mat4, Quat, Vec3};

/// A placement's transform: `FromYawPitchOffset(yaw, offset)` (yaw in degrees about z) with the
/// per-axis scale the shaders read from the transform rows' `w` (placement floats 3..6).
pub fn placement_matrix(position: [f32; 3], scale: [f32; 3], yaw: f32) -> [f32; 16] {
    Mat4::from_scale_rotation_translation(
        Vec3::from(scale),
        Quat::from_rotation_z(yaw.to_radians()),
        Vec3::from(position),
    )
    .to_cols_array()
}
