//! Model material classification and the character outfit atlas, shared by the viewer and the
//! game so both draw a material the same way.

/// Which ported model shader a range uses, from its material instance's base material.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MaterialKind {
    /// `Material\Opaque`, `OpaqueInWater` (`OpaqueMaterial_ps`).
    Opaque,
    /// `Material\OpaqueSnow`: roofs collect snow where the diffuse alpha allows.
    OpaqueSnow,
    /// `Material\AlphaTest`: opaque with a 0.5 alpha cut.
    AlphaTest,
    /// `Material\Foliage`: pines sway with the wind and hold snow on upward faces.
    Foliage,
    /// `Material\Billboard`: leaf cards expanded towards the camera (`BillboardMaterial_vs/ps`).
    Billboard,
    /// `Material\OpaqueSkin\OpaqueCharacterMaterial`: skinned characters textured from an atlas
    /// of [`CHARACTER_OUTFITS`] outfits (`NormalSkin_charactervs`, then `Normal_ps`).
    Character,
}

impl MaterialKind {
    pub fn of(base: Option<&str>) -> MaterialKind {
        let base = base.unwrap_or_default().replace('/', "\\");
        if base.contains("OpaqueCharacterMaterial") {
            MaterialKind::Character
        } else if base.contains("\\Billboard\\") {
            MaterialKind::Billboard
        } else if base.contains("\\Foliage\\") {
            MaterialKind::Foliage
        } else if base.contains("\\OpaqueSnow\\") {
            MaterialKind::OpaqueSnow
        } else if base.contains("\\AlphaTest\\") {
            MaterialKind::AlphaTest
        } else {
            MaterialKind::Opaque
        }
    }
}

/// `NormalSkin_charactervs` (`c5.z = 0.125`): the character texture holds 8 outfits side by side.
pub const CHARACTER_OUTFITS: f32 = 8.0;

/// The character shader's diffuse U: `(u + outfit) / 8`, where `u` is the vertex texcoord
/// (`/512`) and `outfit` (0..7, citizen `+0x34`) rides in the root transform's translation `w`
/// (`c44.w`). As a scale/offset pair for `uv * xy + zw`.
pub fn character_uv_transform(outfit: u32) -> [f32; 4] {
    [
        1.0 / CHARACTER_OUTFITS,
        1.0,
        outfit as f32 / CHARACTER_OUTFITS,
        0.0,
    ]
}
