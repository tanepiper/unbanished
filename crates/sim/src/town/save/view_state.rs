//! What a save stores from the presentation (not the simulation): the camera's direction and
//! the ambience's emitters. The presentation fills these records and the save writes them, so
//! the save code needs nothing from the renderer or the audio.

use glam::Vec3;

/// `Math::Vector3::FromYawPitch` (degrees): the direction a camera with this yaw and pitch faces.
pub fn from_yaw_pitch(yaw: f32, pitch: f32) -> Vec3 {
    let (y, p) = (yaw.to_radians(), pitch.to_radians());
    Vec3::new(-p.cos() * y.sin(), p.cos() * y.cos(), p.sin())
}

/// A region's AmbientEnvironmentComponent and emitter, for saving.
pub struct RegionSave {
    pub rect: (i32, i32, i32, i32),
    pub position: [f32; 3],
    /// The two slots' effects and volumes.
    pub effects: [(Option<String>, f32); 2],
}

/// The map ambience's entities, for saving.
pub struct AmbienceSave {
    pub regions: Vec<RegionSave>,
    pub wind: Option<[f32; 3]>,
    pub rivers: Vec<[f32; 3]>,
    /// The weather emitters while their effect lasts: the ambient file, the position and
    /// the slots' volumes.
    pub weather: Vec<(String, [f32; 3], Vec<f32>)>,
}
