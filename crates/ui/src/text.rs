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
