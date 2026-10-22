//! Sprite sheets and cursors: the named regions of a texture, and a cursor's image and hot spot.

use super::*;

/// A sprite of a sprite sheet.
#[derive(Debug, Clone)]
pub struct Sprite {
    /// The sprite's name.
    pub name: String,
    /// The image file it came from.
    pub source: String,
    /// Offset of the trimmed image inside the original source image.
    pub offset: [i32; 2],
    /// The width in pixels.
    pub width: i32,
    /// The height in pixels.
    pub height: i32,
    /// Not identified yet.
    pub unknown: i32,
    /// u0, v0, u1, v1 in the sheet image.
    pub uv: [f32; 4],
}

/// Original: `UI::SpriteSheet`, Runtime `0x18011ffc0`.
#[derive(Debug, Clone)]
pub struct SpriteSheet {
    /// The material's path.
    pub material_path: String,
    /// The image file of the sheet.
    pub source_image: String,
    /// The sheet's width in pixels.
    pub width: i32,
    /// The sheet's height in pixels.
    pub height: i32,
    /// Not identified yet.
    pub unknown_flag: bool,
    /// The sprites.
    pub sprites: Vec<Sprite>,
    /// Material instance whose `diffuse` texture is the sheet image.
    pub material: Ref,
}

/// Read a [`SpriteSheet`] from the bytes of its resource.
pub fn sprite_sheet(data: &[u8]) -> Result<SpriteSheet> {
    expect(data, "SpriteSheet", |r| {
        r.block(|r, _| {
            let material_path = r.string()?;
            let source_image = r.string()?;
            let width = r.i32()?;
            let height = r.i32()?;
            let unknown_flag = r.bool()?;
            let sprites = (0..r.count()?)
                .map(|_| {
                    let name = r.string()?;
                    let source = r.string()?;
                    let offset = i32s(r)?;
                    let width = r.i32()?;
                    let height = r.i32()?;
                    let unknown = r.i32()?;
                    Ok(Sprite {
                        name,
                        source,
                        offset,
                        width,
                        height,
                        unknown,
                        uv: r.f32s()?,
                    })
                })
                .collect::<Result<_>>()?;
            Ok(SpriteSheet {
                material_path,
                source_image,
                width,
                height,
                unknown_flag,
                sprites,
                material: r.external_ref()?,
            })
        })
    })
}

/// A mouse pointer: the sprite sheet whose image is the pointer, and its hot spot.
///
/// Original: `UI::Cursor::Serialize`, Runtime `0x1800cf980` (version 1).
#[derive(Debug, Clone)]
pub struct Cursor {
    /// The sprite sheet whose image is the pointer.
    pub sprite_sheet: Ref,
    /// The point of the image that is the pointer's position, in pixels.
    pub hotspot: [i32; 2],
}

/// Read a [`Cursor`] from the bytes of its resource.
pub fn cursor(data: &[u8]) -> Result<Cursor> {
    expect(data, "Cursor", |r| {
        r.block(|r, _| {
            Ok(Cursor {
                sprite_sheet: r.external_ref()?,
                hotspot: [r.i32()?, r.i32()?],
            })
        })
    })
}

impl SpriteSheet {
    /// A sprite by name.
    pub fn sprite(&self, name: &str) -> Option<&Sprite> {
        self.sprites.iter().find(|s| s.name == name)
    }
}
