//! Text layout following `UI::Font::ComputeStringLayout` / `ComputeLineLayout` (0x180119860/0x180119500).
//!
//! Glyph metrics are `(a, c, b, d)`: the pen advances by `a` plus the previous glyph's `c`, the glyph
//! is drawn there `b` pixels wide, and `d` raises it relative to the baseline. Lines advance by the
//! font size; `\n` breaks lines and, when wrapping, lines break at the last space before the limit.

use crate::resources::{FontId, Resources};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlacedGlyph {
    /// Glyph index, which is also the sprite index in the font's sheet.
    pub glyph: usize,
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextLayout {
    pub glyphs: Vec<PlacedGlyph>,
    pub width: i32,
    pub height: i32,
}

struct Line {
    glyphs: Vec<PlacedGlyph>,
    width: i32,
}

fn layout_line(res: &Resources, font: FontId, text: &str, y: i32) -> Line {
    let loaded = &res.fonts[font];
    let sprites = loaded.sheet.map(|s| &res.sheets[s].sheet.sprites);
    let (mut x, mut prev_c, mut glyphs) = (0, 0, Vec::new());
    for ch in text.chars() {
        let Some(g) = loaded.font.glyph_index(ch) else {
            continue;
        };
        let [a, c, b, d] = loaded.font.glyphs[g];
        x += a + prev_c;
        let h = sprites.and_then(|s| s.get(g)).map_or(0, |s| s.height);
        glyphs.push(PlacedGlyph {
            glyph: g,
            x,
            y: y + loaded.ascent - d - h,
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
        let height = if text.is_empty() {
            0
        } else {
            lines.len() as i32 * line_height
        };
        TextLayout {
            glyphs,
            width,
            height,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resources::{LoadedFont, LoadedSheet};
    use banished_formats::pkg::Archive;
    use banished_formats::ui::{Font, Sprite, SpriteSheet};
    use std::rc::Rc;

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
        assert_eq!(layout.glyphs.iter().map(|g| g.y).collect::<Vec<_>>(), [3, 6, 19]);
        assert_eq!((layout.width, layout.height), (16, 32));
    }
}
