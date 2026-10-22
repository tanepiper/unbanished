//! Caret-markup documents, traced from Ghidra's DocumentTextElement::GetNextItem, LayoutRow, and
//! Draw.
//!
//! Original: Runtime `0x1800d07b0`, Runtime `0x1800d0430`, Runtime `0x1800d0010`.
mod layout;

use crate::controls::intersect;
use crate::*;
use std::collections::HashMap;
use unbanished_formats::ui;
use unbanished_formats::{
    crc::type_id,
    crs,
    error::{Error, Result},
};

#[derive(Debug, Clone)]
pub struct Document {
    pub(crate) text: String,
    pub(crate) width: i32,
    pub(crate) size: [i32; 2],
    /// A link can occupy several rectangles, one per wrapped text/image run.
    pub(crate) links: Vec<(String, [i32; 4])>,
    pub(crate) hovered_link: Option<usize>,
    pub(crate) pressed_link: Option<usize>,
    fonts: Vec<Option<FontId>>,
    sheets: Vec<Option<SheetId>>,
    colors: Vec<u32>,
    bullets: Vec<Option<SpriteRef>>,
    underline: Option<SpriteRef>,
    link_colors: [u32; 3],
    indent: i32,
    items: Vec<Item>,
    captures: HashMap<String, Capture>,
}

#[derive(Debug, Clone)]
struct Item {
    content: Content,
    pos: [i32; 2],
    size: [i32; 2],
    color: u32,
    link: Option<String>,
}
#[derive(Debug, Clone)]
enum Content {
    Text(TextData),
    Sprite(SpriteRef),
    Capture(Capture),
    Space,
}

#[derive(Debug, Clone)]
struct Capture {
    size: [i32; 2],
    parts: Vec<CapturePart>,
}
#[derive(Debug, Clone)]
struct CapturePart {
    sheet: i32,
    sprite: Option<SpriteRef>,
    rect: [i32; 4],
    color: u32,
}

struct RawCapture {
    sheets: Vec<Option<String>>,
    size: [i32; 2],
    parts: Vec<(i32, String, [i32; 4], u32)>,
    rotated_parts: usize,
}

/// SpriteCapture::Serialize (0x1800dbda0) and its record serializer (0x18004cbd0).
fn read_capture(bytes: &[u8]) -> Result<RawCapture> {
    crs::parse(bytes, |r, h| {
        if h.type_id != type_id("SpriteCapture") {
            return Err(Error::Unsupported("expected SpriteCapture".into()));
        }
        r.block(|r, _| {
            let sheets = (0..r.count()?)
                .map(|_| r.external_ref())
                .collect::<Result<Vec<_>>>()?;
            let size = [r.i32()?, r.i32()?];
            let mut rotated_parts = 0;
            let parts = (0..r.count()?)
                .map(|_| {
                    let sheet = r.i32()?;
                    let name = r.string()?;
                    let rect = [r.i32()?, r.i32()?, r.i32()?, r.i32()?];
                    let angle = r.f32()?;
                    rotated_parts += usize::from(angle != 0.0);
                    let color = r.u32()?;
                    Ok((sheet, name, rect, color))
                })
                .collect::<Result<_>>()?;
            Ok(RawCapture {
                sheets,
                size,
                parts,
                rotated_parts,
            })
        })
    })
    .map(|(_, capture)| capture)
}

fn read_capture_list(bytes: &[u8]) -> Result<Vec<Option<String>>> {
    crs::parse(bytes, |r, h| {
        if h.type_id != type_id("ExternalList") {
            return Err(Error::Unsupported("expected ExternalList".into()));
        }
        r.block(|r, _| (0..r.count()?).map(|_| r.external_ref()).collect())
    })
    .map(|(_, list)| list)
}

fn capture_name(reference: &str) -> String {
    reference
        .split(':')
        .next()
        .unwrap_or(reference)
        .replace('\\', "/")
        .to_ascii_lowercase()
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Word(String),
    Command(char, Option<char>, String),
}

/// Whitespace in documents separates words; explicit ^n and ^p perform line/paragraph breaks.
/// Operands run until whitespace or another caret, exactly as FindNextWhiteSpace (0x1800d1840).
fn tokenize(text: &str) -> Vec<Token> {
    let mut chars = text.chars().peekable();
    let mut out = Vec::new();
    while let Some(ch) = chars.next() {
        if ch.is_ascii_whitespace() {
            continue;
        }
        if ch != '^' {
            let mut word = String::from(ch);
            while chars
                .peek()
                .is_some_and(|c| !c.is_ascii_whitespace() && *c != '^')
            {
                word.push(chars.next().unwrap());
            }
            out.push(Token::Word(word));
            continue;
        }
        let Some(command) = chars.next() else {
            break;
        };
        let suffix = if "fcjvmlai".contains(command) {
            chars.next()
        } else {
            None
        };
        let argument = if command == 'd'
            || command == 'i'
            || (command == 'a' && suffix == Some('b'))
            || (command == 'm' && matches!(suffix, Some('l' | 'r')))
        {
            let mut value = String::new();
            while chars
                .peek()
                .is_some_and(|c| !c.is_ascii_whitespace() && *c != '^')
            {
                value.push(chars.next().unwrap());
            }
            value
        } else {
            String::new()
        };
        out.push(Token::Command(command, suffix, argument));
    }
    out
}

struct Row {
    start: usize,
    x: i32,
    y: i32,
    left: i32,
    right: i32,
    align: char,
    vertical: char,
    font: usize,
    color: usize,
    link: Option<String>,
    restore_left: i32,
}

impl Ui {
    /// Resolve the link fragment index supplied by Event::Link to its document target.
    pub fn document_link(&self, id: NodeId, index: usize) -> Option<&str> {
        match &self.nodes.get(id)?.kind {
            Kind::Control(Control::Document(doc)) => {
                doc.links.get(index).map(|(target, _)| target.as_str())
            }
            _ => None,
        }
    }

    pub(crate) fn load_document(&mut self, spec: &ui::DocumentText) -> Document {
        let fonts = spec
            .fonts
            .iter()
            .map(|f| f.as_deref().and_then(|f| self.res.font(f)))
            .collect();
        let sheets = spec
            .sheets
            .iter()
            .map(|s| s.as_deref().and_then(|s| self.res.sheet(s)))
            .collect();
        let bullets = spec
            .bullets
            .iter()
            .map(|name| self.res.sprite(spec.sprite_sheet.as_deref(), name))
            .collect();
        let mut captures = HashMap::new();
        if let Some(reference) = &spec.images {
            let refs = self
                .res
                .archive
                .read_ref(reference)
                .and_then(|d| read_capture_list(&d));
            match refs {
                Ok(refs) => {
                    for reference in refs.into_iter().flatten() {
                        match self
                            .res
                            .archive
                            .read_ref(&reference)
                            .and_then(|d| read_capture(&d))
                        {
                            Ok(raw) => {
                                if raw.rotated_parts > 0 {
                                    log::warn!("capture {reference}: {} rotated sprites use axis-aligned quads",raw.rotated_parts);
                                }
                                let parts = raw
                                    .parts
                                    .into_iter()
                                    .map(|(sheet, name, rect, color)| CapturePart {
                                        sheet,
                                        sprite: usize::try_from(sheet)
                                            .ok()
                                            .and_then(|i| raw.sheets.get(i))
                                            .and_then(|s| self.res.sprite(s.as_deref(), &name)),
                                        rect,
                                        color,
                                    })
                                    .collect();
                                captures.insert(
                                    capture_name(&reference),
                                    Capture {
                                        size: raw.size,
                                        parts,
                                    },
                                );
                            }
                            Err(e) => log::warn!("document capture {reference}: {e}"),
                        }
                    }
                }
                Err(e) => log::warn!("document capture list {reference}: {e}"),
            }
        }
        Document {
            text: self.res.string(spec.string_table.as_deref(), &spec.text),
            width: spec.width.max(1),
            size: [0, 0],
            links: Vec::new(),
            hovered_link: None,
            pressed_link: None,
            fonts,
            sheets,
            bullets,
            colors: spec.colors.clone(),
            underline: self
                .res
                .sprite(spec.sprite_sheet.as_deref(), &spec.underline_sprite),
            link_colors: spec.link_colors,
            indent: spec.indent,
            items: Vec::new(),
            captures,
        }
    }

    pub(crate) fn draw_document(
        &self,
        doc: &Document,
        origin: [i32; 2],
        color: u32,
        clip: Option<[i32; 4]>,
        out: &mut Vec<Quad>,
    ) {
        let hovered = doc
            .hovered_link
            .and_then(|i| doc.links.get(i))
            .map(|(target, _)| target);
        let pressed = doc
            .pressed_link
            .and_then(|i| doc.links.get(i))
            .map(|(target, _)| target);
        for item in &doc.items {
            let pos = [origin[0] + item.pos[0], origin[1] + item.pos[1]];
            let hot = item
                .link
                .as_ref()
                .is_some_and(|target| Some(target) == hovered);
            let ink = if hot {
                doc.link_colors[if pressed == hovered { 2 } else { 1 }]
            } else {
                item.color
            };
            let tint = modulate(color, ink);
            match &item.content {
                Content::Text(text) => self.draw_text(text, pos, tint, clip, out),
                Content::Sprite(sprite) => Self::sprite_quad(
                    out,
                    sprite,
                    [
                        pos[0],
                        pos[1],
                        pos[0] + sprite.width,
                        pos[1] + sprite.height,
                    ],
                    tint,
                    clip,
                ),
                Content::Capture(capture) => {
                    let mut capture_clip = clip;
                    for part in &capture.parts {
                        let rect = [
                            part.rect[0] + pos[0],
                            part.rect[1] + pos[1],
                            part.rect[2] + pos[0],
                            part.rect[3] + pos[1],
                        ];
                        if part.sheet == -1 {
                            capture_clip = intersect(clip, rect);
                        } else if part.sheet == -2 {
                            capture_clip = clip;
                        } else if let Some(sprite) = part.sprite {
                            Self::sprite_quad(
                                out,
                                &sprite,
                                rect,
                                modulate(color, part.color),
                                capture_clip,
                            );
                        }
                    }
                }
                Content::Space => {}
            }
            if hot {
                if let Some(sprite) = doc.underline {
                    let y = pos[1] + item.size[1] - 1;
                    Self::sprite_quad(
                        out,
                        &sprite,
                        [pos[0], y, pos[0] + item.size[0], y + 1],
                        tint,
                        clip,
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
