//! Text layout following `UI::Font::ComputeStringLayout` / `ComputeLineLayout`.
//!
//! Glyph metrics are `(a, c, b, d)`: the pen advances by `a` plus the previous glyph's `c`, the
//! glyph is drawn there `b` pixels wide, and `d` raises it relative to the baseline. Lines advance
//! by the font size; `\n` breaks lines and, when wrapping, lines break at the last space before the
//! limit.
//!
//! Original: Runtime `0x180119860`, Runtime `0x180119500`.

use crate::resources::{FontId, Resources};
use unbanished_formats::ui::Sprite;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlacedGlyph {
    /// Glyph index, which is also the sprite index in the font's sheet.
    pub glyph: usize,
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Default, PartialEq)]
/// Laid-out text: each glyph's place, and the size of the whole.
pub struct TextLayout {
    /// The glyphs, each with its position.
    pub glyphs: Vec<PlacedGlyph>,
    /// The width in pixels.
    pub width: i32,
    /// The height in pixels.
    pub height: i32,
    /// Distinct input characters absent from the font, rendered using the fallback policy.
    /// Kept even when the font has no replacement glyph, so missing text is diagnosable.
    pub missing_characters: Vec<char>,
}

struct Line {
    glyphs: Vec<PlacedGlyph>,
    width: i32,
}

/// Keep fallback rendering inside the game's atlas. The only synthesized character is a middle
/// dot: its period bitmap is centered on the x-height. All other missing printable characters
/// use the replacement character or question mark, preserving a visible advance during wrapping.
fn resolve_glyph(
    font: &unbanished_formats::ui::Font,
    sprites: Option<&[Sprite]>,
    ch: char,
) -> Option<(usize, i32)> {
    if let Some(glyph) = font.glyph_index(ch) {
        return Some((glyph, 0));
    }
    if ch == '\u{00a0}' {
        if let Some(glyph) = font.glyph_index(' ') {
            return Some((glyph, 0));
        }
    }
    if ch == '·' {
        if let Some(((dot, body), sprites)) = font
            .glyph_index('.')
            .zip(font.glyph_index('x').or_else(|| font.glyph_index('H')))
            .zip(sprites)
        {
            if let Some((dot_sprite, body_sprite)) = sprites.get(dot).zip(sprites.get(body)) {
                let raised_bottom =
                    font.glyphs[body][3] + (body_sprite.height - dot_sprite.height) / 2;
                return Some((dot, raised_bottom - font.glyphs[dot][3]));
            }
        }
    }
    font.glyph_index('\u{fffd}')
        .or_else(|| font.glyph_index('?'))
        .map(|glyph| (glyph, 0))
}

fn layout_line(res: &Resources, font: FontId, text: &str, y: i32) -> Line {
    let loaded = &res.fonts[font];
    let sprites = loaded.sheet.map(|s| &res.sheets[s].sheet.sprites);
    let (mut x, mut prev_c, mut glyphs) = (0, 0, Vec::new());
    for ch in text.chars() {
        let Some((g, raise)) = resolve_glyph(&loaded.font, sprites.map(Vec::as_slice), ch) else {
            continue;
        };
        let [a, c, b, d] = loaded.font.glyphs[g];
        x += a + prev_c;
        let h = sprites.and_then(|s| s.get(g)).map_or(0, |s| s.height);
        glyphs.push(PlacedGlyph {
            glyph: g,
            x,
            y: y + loaded.ascent - d - h - raise,
        });
        x += b;
        prev_c = c;
    }
    Line {
        glyphs,
        width: x + prev_c,
    }
}

fn text_width(res: &Resources, font: FontId, text: &str) -> i32 {
    layout_line(res, font, text, 0).width
}

impl TextLayout {
    /// Lay out `text`. `wrap_width` enables word wrapping at that width; `justify` is 0 left,
    /// 1 centre, 2 right (relative to the widest line).
    pub fn compute(
        res: &Resources,
        font: FontId,
        text: &str,
        wrap_width: Option<i32>,
        justify: u32,
    ) -> TextLayout {
        let mut missing_characters = Vec::new();
        for ch in text.chars().filter(|ch| *ch != '\n') {
            if res.fonts[font].font.glyph_index(ch).is_none() && !missing_characters.contains(&ch) {
                missing_characters.push(ch);
            }
        }
        if !missing_characters.is_empty() {
            log::debug!(
                "font {} uses fallback glyphs for {:?}",
                res.fonts[font].font.name,
                missing_characters
            );
        }
        let line_height = res.fonts[font].font.size;
        let mut lines: Vec<String> = Vec::new();
        for para in text.split('\n') {
            match wrap_width.filter(|w| *w > 0) {
                Some(limit) => {
                    let mut current = String::new();
                    for word in para.split(' ') {
                        let candidate = if current.is_empty() {
                            word.to_string()
                        } else {
                            format!("{current} {word}")
                        };
                        if !current.is_empty() && text_width(res, font, &candidate) > limit {
                            lines.push(std::mem::take(&mut current));
                            current = word.to_string();
                        } else {
                            current = candidate;
                        }
                    }
                    lines.push(current);
                }
                None => lines.push(para.to_string()),
            }
        }
        let laid: Vec<Line> = lines
            .iter()
            .enumerate()
            .map(|(i, l)| layout_line(res, font, l, i as i32 * line_height))
            .collect();
        let width = laid.iter().map(|l| l.width).max().unwrap_or(0);
        let mut glyphs = Vec::new();
        for line in laid {
            let shift = match justify {
                1 => (width - line.width) / 2,
                2 => width - line.width,
                _ => 0,
            };
            glyphs.extend(line.glyphs.into_iter().map(|g| PlacedGlyph {
                x: g.x + shift,
                ..g
            }));
        }
        // `Font::ComputeStringLayout` (0x180119860): unwrapped text is always one line high,
        // even empty; wrapped text has no lines when empty.
        let height = if text.is_empty() && wrap_width.is_some_and(|w| w > 0) {
            0
        } else {
            lines.len() as i32 * line_height
        };
        TextLayout {
            glyphs,
            width,
            height,
            missing_characters,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resources::{LoadedFont, LoadedSheet};
    use std::rc::Rc;
    use unbanished_formats::pkg::Archive;
    use unbanished_formats::ui::{Font, Sprite, SpriteSheet};

    fn resources() -> Resources {
        let mut res = Resources::new(Rc::new(Archive::default()));
        let chars = ['A', 'g', 'x', '.', '?', ' '];
        let mut char_map = vec![-1; 128];
        for (i, ch) in chars.into_iter().enumerate() {
            char_map[ch as usize] = i as i32;
        }
        let font = Font {
            name: "test".into(),
            size: 16,
            sheet_width: 64,
            sheet_height: 64,
            character_set: None,
            sprite_sheet: None,
            material: None,
            char_map,
            glyphs: vec![
                [1, 1, 6, 0],
                [1, 1, 6, -3],
                [1, 1, 5, 0],
                [1, 1, 2, 0],
                [1, 1, 6, 0],
                [0, 0, 4, 0],
            ],
            unknown: [4, 12, 4],
        };
        res.sheets.push(LoadedSheet {
            reference: "test".into(),
            image: None,
            shader: None,
            pixels: None,
            sheet: SpriteSheet {
                material_path: String::new(),
                source_image: String::new(),
                width: 64,
                height: 64,
                unknown_flag: false,
                material: None,
                sprites: [9, 9, 7, 2, 9, 0]
                    .into_iter()
                    .enumerate()
                    .map(|(i, height)| Sprite {
                        name: String::new(),
                        source: String::new(),
                        offset: [0, 0],
                        width: font.glyphs[i][2],
                        height,
                        unknown: 0,
                        uv: [0.0, 0.0, 1.0, 1.0],
                    })
                    .collect(),
            },
        });
        res.fonts.push(LoadedFont {
            ascent: font.baseline_offset(),
            font,
            sheet: Some(0),
        });
        res
    }

    #[test]
    fn uses_serialized_descent_for_baseline_and_line_spacing() {
        let res = resources();
        // Tallest top above baseline is 9, but the serialized baseline is 16 - 4 = 12.
        assert_eq!(res.fonts[0].font.baseline_offset(), 12);
        let layout = TextLayout::compute(&res, 0, "Ag\nA", None, 0);
        assert_eq!(
            layout.glyphs.iter().map(|g| g.y).collect::<Vec<_>>(),
            [3, 6, 19]
        );
        assert_eq!((layout.width, layout.height), (16, 32));
    }

    #[test]
    fn centers_missing_middle_dot_in_x_height_with_original_atlas() {
        let res = resources();
        let layout = TextLayout::compute(&res, 0, "x·x", None, 0);
        assert_eq!(
            layout.glyphs.iter().map(|g| g.glyph).collect::<Vec<_>>(),
            [2, 3, 2]
        );
        // x occupies y=5..12; a two-pixel period centered on it occupies y=8..10.
        assert_eq!(layout.glyphs[1].y, 8);
        assert_eq!(layout.missing_characters, ['·']);
        assert_eq!(layout.width, 18);
    }

    #[test]
    fn missing_characters_remain_visible_and_take_part_in_wrapping() {
        let res = resources();
        let layout = TextLayout::compute(&res, 0, "A ☃☃", Some(20), 0);
        assert_eq!(
            layout.glyphs.iter().map(|g| g.glyph).collect::<Vec<_>>(),
            [0, 4, 4]
        );
        assert_eq!(layout.glyphs[1].y, 19);
        assert_eq!((layout.width, layout.height), (16, 32));
        assert_eq!(layout.missing_characters, ['☃']);
    }

    #[test]
    fn direct_unicode_glyph_takes_precedence_over_fallback() {
        let mut res = resources();
        res.fonts[0].font.char_map.resize(184, -1);
        res.fonts[0].font.char_map['·' as usize] = 4;
        let layout = TextLayout::compute(&res, 0, "·", None, 0);
        assert_eq!(
            layout.glyphs[0],
            PlacedGlyph {
                glyph: 4,
                x: 1,
                y: 3
            }
        );
        assert!(layout.missing_characters.is_empty());
    }

    #[test]
    fn absent_replacement_is_reported_and_nonbreaking_space_keeps_its_advance() {
        let mut res = resources();
        res.fonts[0].font.char_map['?' as usize] = -1;
        let layout = TextLayout::compute(&res, 0, "A\u{a0}☃A", None, 0);
        assert_eq!(
            layout.glyphs.iter().map(|g| g.glyph).collect::<Vec<_>>(),
            [0, 5, 0]
        );
        assert_eq!(layout.width, 20);
        assert_eq!(layout.missing_characters, ['\u{a0}', '☃']);
    }
}
