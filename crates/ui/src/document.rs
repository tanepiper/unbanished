//! Caret-markup documents, traced from Ghidra's DocumentTextElement::GetNextItem
//! (0x1800d07b0), LayoutRow (0x1800d0430), and Draw (0x1800d0010).
use crate::controls::intersect;
use crate::*;
use banished_formats::ui;
use banished_formats::{
    crc::type_id,
    crs,
    error::{Error, Result},
};
use std::collections::HashMap;

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

    pub(crate) fn layout_document(&self, doc: &mut Document) {
        doc.items.clear();
        doc.links.clear();
        let mut row = Row {
            start: 0,
            x: 0,
            y: 0,
            left: 0,
            right: doc.width.max(1),
            align: 'l',
            vertical: 'b',
            font: 0,
            color: 0,
            link: None,
            restore_left: 0,
        };
        let mut columns = [0, 0];
        let tokens = tokenize(&doc.text);
        for (token_index, token) in tokens.iter().cloned().enumerate() {
            match token {
                Token::Word(word) => {
                    let font = doc.fonts.get(row.font).copied().flatten();
                    let text = format!("{word} ");
                    let layout = font
                        .map(|f| TextLayout::compute(&self.res, f, &text, None, 0))
                        .unwrap_or_default();
                    let size = [layout.width, layout.height];
                    let data = TextData {
                        font,
                        text,
                        max_width: 0,
                        justify: 0,
                        wrap: false,
                        layout,
                    };
                    self.document_item(doc, &mut row, Content::Text(data), size);
                }
                Token::Command(command, suffix, arg) => {
                    match (command, suffix) {
                        ('n', _) => self.document_end_row(doc, &mut row),
                        ('p', _) => {
                            self.document_end_row(doc, &mut row);
                            self.document_end_row(doc, &mut row);
                        }
                        ('f', Some(d)) => {
                            if let Some(index) = d.to_digit(10) {
                                if (index as usize) < doc.fonts.len() {
                                    row.font = index as usize;
                                }
                            }
                        }
                        ('c', Some(d)) => {
                            if let Some(index) = d.to_digit(10) {
                                if (index as usize) < doc.colors.len() {
                                    row.color = index as usize;
                                }
                            }
                        }
                        ('j', Some(j @ ('l' | 'r' | 'c'))) => {
                            if row.start < doc.items.len() {
                                self.document_end_row(doc, &mut row);
                            }
                            row.align = j;
                        }
                        ('v', Some(v @ ('t' | 'c' | 'b'))) => {
                            if row.start < doc.items.len() {
                                self.document_end_row(doc, &mut row);
                            }
                            row.vertical = v;
                        }
                        ('m', Some('l' | 'r')) => {
                            if row.start < doc.items.len() {
                                self.document_end_row(doc, &mut row);
                            }
                            let value = arg.parse::<i32>().unwrap_or(0);
                            if suffix == Some('l') {
                                row.left = value.max(0);
                                row.x = row.left;
                            } else {
                                row.right = if value < 0 { doc.width } else { value };
                            }
                        }
                        ('m', Some('b' | 'c' | 'e')) => {
                            if row.start < doc.items.len() {
                                self.document_end_row(doc, &mut row);
                            }
                            match suffix {
                                Some('b') => columns = [row.y, row.y],
                                Some('c') => {
                                    columns[1] = columns[1].max(row.y);
                                    row.y = columns[0];
                                }
                                _ => {
                                    columns[1] = columns[1].max(row.y);
                                    row.y = columns[1];
                                }
                            }
                            row.x = row.left;
                        }
                        ('l', Some('b' | 'e')) => {
                            if row.start < doc.items.len() {
                                self.document_end_row(doc, &mut row);
                            }
                            row.left = if suffix == Some('b') {
                                row.left + doc.indent
                            } else {
                                row.left - doc.indent
                            }
                            .max(0);
                            row.x = row.left;
                        }
                        ('l', Some('i')) => {
                            if row.start < doc.items.len() {
                                self.document_end_row(doc, &mut row);
                            }
                            if let Some(sprite) = doc.bullets.get(row.font).copied().flatten() {
                                row.left -= sprite.width;
                                row.x = row.left;
                                row.restore_left = sprite.width;
                                self.document_item(
                                    doc,
                                    &mut row,
                                    Content::Sprite(sprite),
                                    [sprite.width, sprite.height],
                                );
                            }
                        }
                        ('l', Some('o')) => {
                            if row.start < doc.items.len() {
                                self.document_end_row(doc, &mut row);
                            }
                            // GetNextItem previews the next item's width for a hanging list marker.
                            let font = doc.fonts.get(row.font).copied().flatten();
                            let marker_width = tokens[token_index + 1..]
                                .iter()
                                .find_map(|token| match token {
                                    Token::Word(word) => font.map(|f| {
                                        TextLayout::compute(
                                            &self.res,
                                            f,
                                            &format!("{word} "),
                                            None,
                                            0,
                                        )
                                        .width
                                    }),
                                    _ => None,
                                })
                                .unwrap_or(doc.indent);
                            row.y += font.map_or(0, |f| self.res.fonts[f].font.size / 4);
                            row.left -= marker_width;
                            row.x = row.left;
                            row.restore_left = marker_width;
                        }
                        ('a', Some('b')) => row.link = (!arg.is_empty()).then_some(arg),
                        ('a', Some('e')) => row.link = None,
                        ('i', Some(d)) => {
                            if let Some(sheet) = d
                                .to_digit(10)
                                .and_then(|i| doc.sheets.get(i as usize))
                                .copied()
                                .flatten()
                            {
                                if let Some(s) = self.res.sheets[sheet].sheet.sprite(&arg) {
                                    let sprite = SpriteRef {
                                        sheet,
                                        width: s.width,
                                        height: s.height,
                                        uv: s.uv,
                                    };
                                    self.document_item(
                                        doc,
                                        &mut row,
                                        Content::Sprite(sprite),
                                        [s.width, s.height],
                                    );
                                }
                            }
                        }
                        ('t', _) => {
                            let width = doc.indent;
                            self.document_item(doc, &mut row, Content::Space, [width, 0]);
                        }
                        ('s', _) => {
                            let width = doc.fonts.get(row.font).copied().flatten().map_or(0, |f| {
                                TextLayout::compute(&self.res, f, " ", None, 0).width
                            });
                            self.document_item(doc, &mut row, Content::Space, [width, 0]);
                        }
                        ('d', _) => {
                            if let Some(capture) = doc.captures.get(&capture_name(&arg)).cloned() {
                                let size = capture.size;
                                self.document_item(doc, &mut row, Content::Capture(capture), size);
                            } else {
                                log::warn!("document capture {arg:?} was not found in its image collection");
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        if row.start < doc.items.len() {
            self.document_end_row(doc, &mut row);
        }
        doc.size = [doc.width.max(1), row.y];
        for item in &doc.items {
            if let Some(target) = &item.link {
                doc.links.push((
                    target.clone(),
                    [
                        item.pos[0],
                        item.pos[1],
                        item.pos[0] + item.size[0],
                        item.pos[1] + item.size[1],
                    ],
                ));
            }
        }
    }

    fn document_item(&self, doc: &mut Document, row: &mut Row, content: Content, size: [i32; 2]) {
        if row.start < doc.items.len() && row.x + size[0] > row.right {
            self.document_end_row(doc, row);
        }
        let color = if row.link.is_some() {
            doc.link_colors[0]
        } else {
            doc.colors.get(row.color).copied().unwrap_or(0xFFFF_FFFF)
        };
        doc.items.push(Item {
            content,
            pos: [row.x, row.y],
            size,
            color,
            link: row.link.clone(),
        });
        row.x += size[0];
    }

    fn document_end_row(&self, doc: &mut Document, row: &mut Row) {
        let default_height = doc
            .fonts
            .get(row.font)
            .copied()
            .flatten()
            .map_or(0, |f| self.res.fonts[f].font.size);
        let height = doc.items[row.start..]
            .iter()
            .map(|i| i.size[1])
            .max()
            .unwrap_or(default_height);
        let width = row.x - row.left;
        let shift = match row.align {
            'r' => row.right - row.left - width,
            'c' => (row.right - row.left - width) / 2,
            _ => 0,
        };
        for item in &mut doc.items[row.start..] {
            item.pos[0] += shift;
            item.pos[1] = row.y
                + match row.vertical {
                    't' => 0,
                    'c' => (height - item.size[1]) / 2,
                    _ => height - item.size[1],
                };
        }
        row.y += height;
        row.start = doc.items.len();
        row.left += row.restore_left;
        row.restore_left = 0;
        row.x = row.left;
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
mod tests {
    use super::*;
    use banished_formats::pkg::Package;
    #[test]
    fn parses_native_caret_commands_and_unicode_without_losing_words() {
        assert_eq!(
            tokenize("^f1Title^p^c2^abhelp.topic Link · text^ae ^i0Icon ^ml20 ^jl next"),
            vec![
                Token::Command('f', Some('1'), String::new()),
                Token::Word("Title".into()),
                Token::Command('p', None, String::new()),
                Token::Command('c', Some('2'), String::new()),
                Token::Command('a', Some('b'), "help.topic".into()),
                Token::Word("Link".into()),
                Token::Word("·".into()),
                Token::Word("text".into()),
                Token::Command('a', Some('e'), String::new()),
                Token::Command('i', Some('0'), "Icon".into()),
                Token::Command('m', Some('l'), "20".into()),
                Token::Command('j', Some('l'), String::new()),
                Token::Word("next".into())
            ]
        );
        assert!(tokenize("^f").len() == 1);
    }

    #[test]
    fn renders_shipped_help_documents_and_decodes_captures_when_available() {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../game-data/original/WinData");
        if !dir.join("data0.pkg").exists() {
            eprintln!("game data unavailable; skipping rich document integration");
            return;
        }
        let mut archive = Archive::default();
        for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
            archive.add(Package::parse(std::fs::read(dir.join(name)).unwrap()).unwrap());
        }
        let archive = Rc::new(archive);
        let mut specs = Vec::new();
        let mut capture_count = 0;
        let mut rotated_parts = 0;
        let mut rich_texts = Vec::new();
        for name in archive.names().into_iter().filter(|n| n.ends_with(".crs")) {
            let bytes = archive.read(name).unwrap();
            match crs::header(&bytes).unwrap().type_id {
                id if id == type_id("DocumentTextDescription") => {
                    let element = ui::element(&bytes).unwrap();
                    if let ui::ElementKind::DocumentText(spec) = element.kind {
                        specs.push((name.to_string(), spec));
                    }
                }
                id if id == type_id("SpriteCapture") => {
                    let capture = read_capture(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
                    rotated_parts += capture.rotated_parts;
                    capture_count += 1;
                }
                id if id == type_id("StringTable") => {
                    rich_texts.extend(
                        ui::string_table(&bytes)
                            .unwrap()
                            .into_iter()
                            .filter(|(_, text)| text.contains('^')),
                    );
                }
                _ => {}
            }
        }
        assert!(!specs.is_empty());
        let mut ui = Ui::new(archive.clone());
        let mut rendered = 0;
        let mut linked = 0;
        let mut embedded = 0;
        for (name, spec) in specs {
            let mut doc = ui.load_document(&spec);
            assert!(
                doc.fonts.iter().all(Option::is_some),
                "{name}: missing document font"
            );
            assert!(
                doc.sheets.iter().all(Option::is_some),
                "{name}: missing document sprite sheet"
            );
            assert!(
                doc.captures
                    .values()
                    .flat_map(|c| &c.parts)
                    .all(|part| part.sheet < 0 || part.sprite.is_some()),
                "{name}: unresolved capture sprite"
            );
            eprintln!(
                "{name}: {} fonts, {} sheets, {} captures; width {}",
                doc.fonts.len(),
                doc.sheets.len(),
                doc.captures.len(),
                doc.width
            );
            if let Some(reference) = &spec.images {
                let refs = read_capture_list(&archive.read_ref(reference).unwrap()).unwrap();
                assert_eq!(
                    doc.captures.len(),
                    refs.iter().flatten().count(),
                    "{name}: failed to load capture collection"
                );
            }
            let mut texts = spec
                .string_table
                .as_deref()
                .map(|table| ui::string_table(&archive.read_ref(table).unwrap()).unwrap())
                .unwrap_or_else(|| vec![(String::new(), spec.text.clone())]);
            if name.contains("Help_documentHelp") {
                texts.extend(rich_texts.clone());
            }
            for (key, text) in texts {
                if text.is_empty() {
                    continue;
                }
                doc.text = text;
                ui.layout_document(&mut doc);
                let mut quads = Vec::new();
                ui.draw_document(&doc, [0, 0], 0xFFFF_FFFF, None, &mut quads);
                assert!(!quads.is_empty(), "{name} {key}: no document quads");
                assert!(doc.size[1] > 0, "{name} {key}: zero document height");
                assert!(
                    doc.links.iter().all(|(_, r)| r[0] < r[2] && r[1] < r[3]),
                    "{name} {key}: invalid link rectangle"
                );
                linked += doc.links.len();
                embedded += doc
                    .items
                    .iter()
                    .filter(|i| matches!(i.content, Content::Sprite(_) | Content::Capture(_)))
                    .count();
                rendered += 1;
            }
            if let Some(capture) = doc.captures.keys().next().cloned() {
                doc.text = format!("^d{capture}");
                ui.layout_document(&mut doc);
                let mut quads = Vec::new();
                ui.draw_document(&doc, [0, 0], 0xFFFF_FFFF, None, &mut quads);
                assert!(!quads.is_empty(), "{name}: capture generates no sprites");
                embedded += 1;
            }
            if let Some((slot, sprite_name)) =
                doc.sheets.iter().enumerate().find_map(|(slot, s)| {
                    s.and_then(|s| {
                        ui.res.sheets[s]
                            .sheet
                            .sprites
                            .first()
                            .map(|s| (slot, s.name.clone()))
                    })
                })
            {
                doc.text = format!("^i{slot}{sprite_name}");
                ui.layout_document(&mut doc);
                let mut quads = Vec::new();
                ui.draw_document(&doc, [0, 0], 0xFFFF_FFFF, None, &mut quads);
                assert_eq!(quads.len(), 1, "{name}: inline sprite generates no quad");
                embedded += 1;
            }
            if name.contains("Help_documentHelp") {
                doc.width = 40;
                doc.text = "^abHelpTopic One Two Three Four^ae".into();
                ui.layout_document(&mut doc);
                assert!(doc.links.len() >= 2);
                assert!(doc.links.iter().all(|(target, _)| target == "HelpTopic"));
                assert!(
                    doc.links.iter().any(|(_, rect)| rect[1] > 0),
                    "link did not wrap"
                );
                let mut normal = Vec::new();
                ui.draw_document(&doc, [0, 0], 0xFFFF_FFFF, None, &mut normal);
                doc.hovered_link = Some(0);
                let mut hovered = Vec::new();
                ui.draw_document(&doc, [0, 0], 0xFFFF_FFFF, None, &mut hovered);
                assert!(
                    hovered.len() > normal.len(),
                    "hover did not add the configured underline"
                );
                assert!(hovered
                    .iter()
                    .any(|q| q.color == argb_to_rgba(doc.link_colors[1], 255)));
                doc.pressed_link = Some(0);
                let mut pressed = Vec::new();
                ui.draw_document(&doc, [0, 0], 0xFFFF_FFFF, None, &mut pressed);
                assert!(pressed
                    .iter()
                    .any(|q| q.color == argb_to_rgba(doc.link_colors[2], 255)));
            }
        }
        eprintln!("rendered {rendered} shipped document strings, {linked} link rectangles, {embedded} embedded images; decoded {capture_count} captures");
        assert!(rendered > 0);
        assert!(linked > 0);
        assert_eq!(
            rotated_parts, 0,
            "shipped captures require rotation support"
        );
    }
}
