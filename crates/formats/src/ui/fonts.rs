//! Fonts and string tables: a font's glyphs and metrics, and a table's keyed strings.

use super::*;

/// `UI::Font`, block version 2.
///
/// Original: Runtime `0x180119f60`.
#[derive(Debug, Clone)]
pub struct Font {
    /// The font's name.
    pub name: String,
    /// The font's size, in pixels.
    pub size: i32,
    /// The glyph sheet's width.
    pub sheet_width: i32,
    /// The glyph sheet's height.
    pub sheet_height: i32,
    /// The character set resource.
    pub character_set: Ref,
    /// Sprite sheet holding one sprite per glyph, in glyph order.
    pub sprite_sheet: Ref,
    /// The material the glyphs are drawn with.
    pub material: Ref,
    /// Code point -> glyph index; negative or 0xffff means no glyph.
    pub char_map: Vec<i32>,
    /// Per glyph: (a, c, b, d): advance = a + b, with the previous glyph's c added;
    /// d raises the bottom of the sprite above the baseline.
    pub glyphs: Vec<[i32; 4]>,
    // +0x54
    /// Serialized descent, ellipsis advance, and space advance (Runtime Font/58/5c).
    pub unknown: [i32; 3],
}

/// Read a [`Font`] from the bytes of its resource.
pub fn font(data: &[u8]) -> Result<Font> {
    expect(data, "Font", |r| {
        r.block(|r, version| {
            if version < 2 {
                return Err(Error::Unsupported("Font block version < 2".into()));
            }
            let name = r.string()?;
            let size = r.i32()?;
            let _sheet_path = r.string()?;
            let sheet_width = r.i32()?;
            let sheet_height = r.i32()?;
            let _material_path = r.string()?;
            let _image_path = r.string()?;
            let character_set = r.external_ref()?;
            let sprite_sheet = r.external_ref()?;
            let material = r.external_ref()?;
            let char_map = (0..r.count()?).map(|_| r.i32()).collect::<Result<_>>()?;
            let glyphs = (0..r.count()?).map(|_| i32s(r)).collect::<Result<_>>()?;
            Ok(Font {
                name,
                size,
                sheet_width,
                sheet_height,
                character_set,
                sprite_sheet,
                material,
                char_map,
                glyphs,
                unknown: i32s(r)?,
            })
        })
    })
}

/// Key -> display string.
///
/// Original: `UI::StringTable`, Runtime `0x1801201d0`.
pub fn string_table(data: &[u8]) -> Result<Vec<(String, String)>> {
    expect(data, "StringTable", |r| {
        r.block(|r, _| {
            (0..r.count()?)
                .map(|_| Ok((r.string()?, r.string()?)))
                .collect()
        })
    })
}

impl Font {
    /// Baseline relative to a text element's top, as used by TextElement::Draw.
    ///
    /// Original: Runtime `0x180121b10`.
    pub fn baseline_offset(&self) -> i32 {
        self.size - self.unknown[0]
    }

    /// The glyph of a character.
    pub fn glyph_index(&self, ch: char) -> Option<usize> {
        let g = *self.char_map.get(ch as usize)?;
        (g >= 0 && g != 0xFFFF && (g as usize) < self.glyphs.len()).then_some(g as usize)
    }
}
