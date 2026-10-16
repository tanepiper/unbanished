//! `UI::` resources: dialogs, element descriptions, fonts, sprite sheets and string tables.
//!
//! Every class level of `Serialize` is its own block and derived classes serialize their base first.
//! Field meanings come from `ElementDescription` defaults, `Element::Element` and the per-class
//! `ComputeSize`/`ComputeLayout` code; names marked "unknown" have not been traced to behaviour yet.

use crate::crc::type_id;
use crate::crs::{self, Reader};
use crate::error::{format_err, Error, Result};

type Ref = Option<String>;

fn refs(r: &mut Reader) -> Result<Vec<Ref>> {
    (0..r.count()?).map(|_| r.external_ref()).collect()
}

fn strings<const N: usize>(r: &mut Reader) -> Result<[String; N]> {
    let mut out: [String; N] = std::array::from_fn(|_| String::new());
    for s in &mut out {
        *s = r.string()?;
    }
    Ok(out)
}

fn i32s<const N: usize>(r: &mut Reader) -> Result<[i32; N]> {
    let mut out = [0; N];
    for v in &mut out {
        *v = r.i32()?;
    }
    Ok(out)
}

/// Alignment within the space a parent offers: 0..8 = rows top/middle/bottom x columns left/centre/right.
pub type Alignment = u32;

pub mod flags {
    pub const HIDDEN: u32 = 1 << 0;
    pub const FILL_WIDTH: u32 = 1 << 2;
    pub const FILL_HEIGHT: u32 = 1 << 3;
    /// Height (0x10) / width (0x20) taken from the screen size times the UI scale.
    pub const SCREEN_HEIGHT: u32 = 1 << 4;
    pub const SCREEN_WIDTH: u32 = 1 << 5;
    /// Halves the element's alpha (`Element::ComputeColor`).
    pub const DIM: u32 = 1 << 18;
    /// Takes the mouse (`SetFlagsOnOff(0x10000)`): a click on it is an event of its own.
    pub const MOUSE: u32 = 1 << 16;
    /// Disabled by the tutorial (`FUN_1400e7720`; input passes it by, like a disabled one).
    pub const LOCKED: u32 = 1 << 24;
}

/// `UI::ElementDescription` (0x180119390): shared by every element.
#[derive(Debug, Clone, Default)]
pub struct ElementBase {
    /// left, right, top, bottom
    pub margins: [i32; 4],
    pub min_size: [i32; 2],
    pub alignment: Alignment,
    pub flags: u32,
    /// ARGB, multiplied into everything the element draws.
    pub color: u32,
    /// `+0x3c`: the mouse cursor shown over it (`Element::GetMouseCursorIndex`; an index into
    /// the game's cursors: 0 Arrow, 1 Hand, 2 Hourglass, 3 Move, 4 Query, 5 Place). Only the
    /// StandardDialog title bar sets one (3).
    pub cursor: i32,
    pub border: Ref,
    pub tooltip_dialog: Ref,
    pub string_table: Ref,
    pub tooltip: String,
}

impl ElementBase {
    fn read(r: &mut Reader) -> Result<Self> {
        r.block(|r, _| {
            let margins = i32s(r)?;
            let min_size = i32s(r)?;
            let alignment = r.u32()?;
            let flags = r.u32()?;
            let color = r.u32()?;
            let cursor = r.i32()?;
            Ok(ElementBase {
                margins,
                min_size,
                alignment,
                flags,
                color,
                cursor,
                border: r.external_ref()?,
                tooltip_dialog: r.external_ref()?,
                string_table: r.external_ref()?,
                tooltip: r.string()?,
            })
        })
    }
}

#[derive(Debug, Clone)]
pub struct Button {
    /// enabled, disabled, highlight, pressed state elements
    pub states: [Ref; 4],
    pub content: Ref,
    /// Content colours per state (ARGB), same order as `states`.
    pub colors: [u32; 4],
}

#[derive(Debug, Clone)]
pub struct Text {
    pub font: Ref,
    pub string_table: Ref,
    /// String-table key, or literal text when no table is given.
    pub text: String,
    /// Draw a black shadow one pixel down and right (TextElement::Draw).
    pub shadow: bool,
    pub wrap: bool,
    /// Width limit passed to `Font::ComputeStringLayout` (wrapping / ellipsis).
    pub max_width: i32,
    /// `Font::Justify`: 0 left, 1 centre, 2 right.
    pub justify: u32,
    /// Non-wrapped line width at which the engine inserts "...". This is not a key code.
    pub ellipsis_width: i32,
}

#[derive(Debug, Clone)]
pub struct Label {
    pub text: Text,
    pub sprite_sheet: Ref,
    pub sprite: String,
    pub text_alignment: u32,
    /// > 1 stacks icon above text, otherwise icon left of text (`LabelElement::ComputeSize`).
    pub orientation: u32,
    pub min_text_width: i32,
    pub gap: i32,
    pub icon_size: [i32; 2],
}

#[derive(Debug, Clone)]
pub enum ElementKind {
    Group {
        children: Vec<Ref>,
    },
    Page {
        children: Vec<Ref>,
        page: i32,
    },
    Radio {
        children: Vec<Ref>,
        selected: i32,
    },
    Ribbon {
        vertical: bool,
        uniform: bool,
        spacing: i32,
        children: Vec<Ref>,
    },
    Border {
        sprite_sheet: Ref,
        sprites: [String; 9],
    },
    Image {
        sprite_sheet: Ref,
        sprite: String,
        size: [i32; 2],
    },
    /// The game's `FrustumOutlineDescription` (MapUI's camera outline): an image block whose
    /// element draws lines instead of the sprite (`FUN_14006c7f0`).
    FrustumOutline {
        sprite_sheet: Ref,
        sprite: String,
        size: [i32; 2],
    },
    AnimatedImage {
        sprite_sheet: Ref,
        sprite: String,
        size: [i32; 2],
        frames: Vec<String>,
        rate: i32,
    },
    Button(Button),
    Check {
        button: Button,
        check: Ref,
    },
    Combo {
        button: Button,
        refs: [Ref; 3],
    },
    Text(Text),
    Label(Label),
    ClipRegion {
        unknown: [i32; 2],
        content: Ref,
    },
    Container {
        unknown: [i32; 3],
        refs: [Ref; 3],
        unknown_bools: [bool; 2],
    },
    Edit {
        font: Ref,
        text: String,
        max_length: i32,
        unknown_bool: bool,
        unknown_string: String,
    },
    Graph {
        unknown: [i32; 3],
        unknown_u32: u32,
        sprite_sheet: Ref,
        sprite: String,
    },
    Layout {
        items: Vec<LayoutItem>,
    },
    List {
        refs: [Ref; 2],
        sprite_sheet: Ref,
        sprite: String,
        unknown: [u32; 2],
    },
    NumberEdit {
        font: Ref,
        unknown: [i32; 4],
        unknown_bool: bool,
    },
    Number {
        font: Ref,
        color: u32,
        unknown_int: i32,
        unknown_float: f32,
        unknown_bool: bool,
    },
    Slider {
        track: Ref,
        sprite_sheet: Ref,
        sprites: [String; 5],
        unknown: [i32; 2],
        range: [f32; 2],
        unknown_b0: i32,
    },
    Spinner {
        refs: [Ref; 3],
        unknown: i32,
    },
    Tab {
        refs: [Ref; 3],
        sprite_sheet: Ref,
        sprites: [String; 3],
        unknown: i32,
        tabs: Vec<(Ref, Ref)>,
    },
    Progress {
        color: u32,
        sprite_sheet: Ref,
        sprites: [String; 6],
        unknown_ref: Ref,
    },
    Scroll {
        sprite_sheet: Ref,
        sprites: [String; 9],
        vertical: bool,
        unknown: i32,
        unknown_u32: [u32; 2],
    },
    Table {
        unknown: [i32; 3],
        cells: Vec<TableCell>,
    },
    DocumentText(DocumentText),
}

/// Rich text configuration (`DocumentTextDescription::Serialize`, 0x1800d2110).
#[derive(Debug, Clone)]
pub struct DocumentText {
    pub string_table: Ref,
    pub text: String,
    pub sprite_sheet: Ref,
    pub bullets: Vec<String>,
    pub underline_sprite: String,
    pub link_colors: [u32; 3],
    pub indent: i32,
    pub width: i32,
    pub fonts: Vec<Ref>,
    pub sheets: Vec<Ref>,
    pub colors: Vec<u32>,
    pub images: Ref,
}

#[derive(Debug, Clone)]
pub struct LayoutItem {
    pub element: Ref,
    pub strings: [String; 3],
    pub unknown_u32: [u32; 2],
    pub unknown_i32: [i32; 2],
    pub flag: bool,
}

#[derive(Debug, Clone)]
pub struct TableCell {
    pub column: i32,
    pub row: i32,
    pub element: Ref,
}

#[derive(Debug, Clone)]
pub struct Element {
    pub class: &'static str,
    pub base: ElementBase,
    pub kind: ElementKind,
}

impl Element {
    pub fn children(&self) -> &[Ref] {
        match &self.kind {
            ElementKind::Group { children }
            | ElementKind::Page { children, .. }
            | ElementKind::Radio { children, .. }
            | ElementKind::Ribbon { children, .. } => children,
            _ => &[],
        }
    }
}

fn read_button(r: &mut Reader) -> Result<Button> {
    r.block(|r, _| {
        let mut states: [Ref; 4] = Default::default();
        for s in &mut states {
            *s = r.external_ref()?;
        }
        let content = r.external_ref()?;
        let colors = [r.u32()?, r.u32()?, r.u32()?, r.u32()?];
        Ok(Button {
            states,
            content,
            colors,
        })
    })
}

fn read_text(r: &mut Reader) -> Result<Text> {
    r.block(|r, _| {
        Ok(Text {
            font: r.external_ref()?,
            string_table: r.external_ref()?,
            text: r.string()?,
            shadow: r.bool()?,
            wrap: r.bool()?,
            max_width: r.i32()?,
            justify: r.u32()?,
            ellipsis_width: r.i32()?,
        })
    })
}

fn read_group(r: &mut Reader) -> Result<Vec<Ref>> {
    r.block(|r, _| refs(r))
}

fn read_image(r: &mut Reader) -> Result<(Ref, String, [i32; 2])> {
    r.block(|r, _| Ok((r.external_ref()?, r.string()?, i32s(r)?)))
}

/// Decode any `UI::*Description` element given its class name.
fn read_element(class: &'static str, r: &mut Reader) -> Result<Element> {
    let base = ElementBase::read(r)?;
    let kind = match class {
        "GroupDescription" => ElementKind::Group {
            children: read_group(r)?,
        },
        "PageDescription" => {
            let children = read_group(r)?;
            ElementKind::Page {
                children,
                page: r.block(|r, _| r.i32())?,
            }
        }
        "RadioDescription" => {
            let children = read_group(r)?;
            ElementKind::Radio {
                children,
                selected: r.block(|r, _| r.i32())?,
            }
        }
        "RibbonDescription" => r.block(|r, _| {
            Ok(ElementKind::Ribbon {
                vertical: r.bool()?,
                uniform: r.bool()?,
                spacing: r.i32()?,
                children: refs(r)?,
            })
        })?,
        "BorderDescription" => r.block(|r, _| {
            Ok(ElementKind::Border {
                sprite_sheet: r.external_ref()?,
                sprites: strings(r)?,
            })
        })?,
        "ImageDescription" => {
            let (sprite_sheet, sprite, size) = read_image(r)?;
            ElementKind::Image {
                sprite_sheet,
                sprite,
                size,
            }
        }
        "FrustumOutlineDescription" => {
            let (sprite_sheet, sprite, size) = read_image(r)?;
            ElementKind::FrustumOutline {
                sprite_sheet,
                sprite,
                size,
            }
        }
        "AnimatedImageDescription" => {
            let (sprite_sheet, sprite, size) = read_image(r)?;
            r.block(|r, _| {
                let frames = (0..r.count()?).map(|_| r.string()).collect::<Result<_>>()?;
                Ok(ElementKind::AnimatedImage {
                    sprite_sheet,
                    sprite,
                    size,
                    frames,
                    rate: r.i32()?,
                })
            })?
        }
        "ButtonDescription" => ElementKind::Button(read_button(r)?),
        "CheckDescription" => {
            let button = read_button(r)?;
            ElementKind::Check {
                button,
                check: r.block(|r, _| r.external_ref())?,
            }
        }
        "ComboDescription" => {
            let button = read_button(r)?;
            r.block(|r, _| {
                Ok(ElementKind::Combo {
                    button,
                    refs: [r.external_ref()?, r.external_ref()?, r.external_ref()?],
                })
            })?
        }
        "TextDescription" => ElementKind::Text(read_text(r)?),
        "LabelDescription" => {
            let text = read_text(r)?;
            r.block(|r, _| {
                let sprite_sheet = r.external_ref()?;
                let sprite = r.string()?;
                let text_alignment = r.u32()?;
                let orientation = r.u32()?;
                let [min_text_width, gap, w, h] = i32s(r)?;
                Ok(ElementKind::Label(Label {
                    text,
                    sprite_sheet,
                    sprite,
                    text_alignment,
                    orientation,
                    min_text_width,
                    gap,
                    icon_size: [w, h],
                }))
            })?
        }
        "ClipRegionDescription" => r.block(|r, _| {
            Ok(ElementKind::ClipRegion {
                unknown: i32s(r)?,
                content: r.external_ref()?,
            })
        })?,
        "ContainerDescription" => r.block(|r, _| {
            Ok(ElementKind::Container {
                unknown: i32s(r)?,
                refs: [r.external_ref()?, r.external_ref()?, r.external_ref()?],
                unknown_bools: [r.bool()?, r.bool()?],
            })
        })?,
        "EditDescription" => r.block(|r, _| {
            Ok(ElementKind::Edit {
                font: r.external_ref()?,
                text: r.string()?,
                max_length: r.i32()?,
                unknown_bool: r.bool()?,
                unknown_string: r.string()?,
            })
        })?,
        "GraphDescription" => r.block(|r, _| {
            Ok(ElementKind::Graph {
                unknown: i32s(r)?,
                unknown_u32: r.u32()?,
                sprite_sheet: r.external_ref()?,
                sprite: r.string()?,
            })
        })?,
        "LayoutDescription" => r.block(|r, _| {
            let items = (0..r.count()?)
                .map(|_| {
                    Ok(LayoutItem {
                        element: r.external_ref()?,
                        strings: strings(r)?,
                        unknown_u32: [r.u32()?, r.u32()?],
                        unknown_i32: i32s(r)?,
                        flag: r.bool()?,
                    })
                })
                .collect::<Result<_>>()?;
            Ok(ElementKind::Layout { items })
        })?,
        "ListDescription" => r.block(|r, _| {
            Ok(ElementKind::List {
                refs: [r.external_ref()?, r.external_ref()?],
                sprite_sheet: r.external_ref()?,
                sprite: r.string()?,
                unknown: [r.u32()?, r.u32()?],
            })
        })?,
        "NumberEditDescription" => r.block(|r, _| {
            Ok(ElementKind::NumberEdit {
                font: r.external_ref()?,
                unknown: i32s(r)?,
                unknown_bool: r.bool()?,
            })
        })?,
        "NumberDescription" => r.block(|r, _| {
            Ok(ElementKind::Number {
                font: r.external_ref()?,
                color: r.u32()?,
                unknown_int: r.i32()?,
                unknown_float: r.f32()?,
                unknown_bool: r.bool()?,
            })
        })?,
        "SliderDescription" => r.block(|r, _| {
            Ok(ElementKind::Slider {
                track: r.external_ref()?,
                sprite_sheet: r.external_ref()?,
                sprites: strings(r)?,
                unknown: i32s(r)?,
                range: [r.f32()?, r.f32()?],
                unknown_b0: r.i32()?,
            })
        })?,
        "SpinnerDescription" => r.block(|r, _| {
            Ok(ElementKind::Spinner {
                refs: [r.external_ref()?, r.external_ref()?, r.external_ref()?],
                unknown: r.i32()?,
            })
        })?,
        "TabDescription" => r.block(|r, _| {
            Ok(ElementKind::Tab {
                refs: [r.external_ref()?, r.external_ref()?, r.external_ref()?],
                sprite_sheet: r.external_ref()?,
                sprites: strings(r)?,
                unknown: r.i32()?,
                tabs: (0..r.count()?)
                    .map(|_| Ok((r.external_ref()?, r.external_ref()?)))
                    .collect::<Result<_>>()?,
            })
        })?,
        "ProgressDescription" => r.block(|r, _| {
            Ok(ElementKind::Progress {
                color: r.u32()?,
                sprite_sheet: r.external_ref()?,
                sprites: strings(r)?,
                unknown_ref: r.external_ref()?,
            })
        })?,
        "ScrollDescription" => r.block(|r, _| {
            Ok(ElementKind::Scroll {
                sprite_sheet: r.external_ref()?,
                sprites: strings(r)?,
                vertical: r.bool()?,
                unknown: r.i32()?,
                unknown_u32: [r.u32()?, r.u32()?],
            })
        })?,
        "TableDescription" => r.block(|r, _| {
            let unknown = i32s(r)?;
            let cells = (0..r.count()?)
                .map(|_| {
                    Ok(TableCell {
                        column: r.i32()?,
                        row: r.i32()?,
                        element: r.external_ref()?,
                    })
                })
                .collect::<Result<_>>()?;
            Ok(ElementKind::Table { unknown, cells })
        })?,
        "DocumentTextDescription" => r.block(|r, _| {
            Ok(ElementKind::DocumentText(DocumentText {
                string_table: r.external_ref()?,
                text: r.string()?,
                sprite_sheet: r.external_ref()?,
                bullets: (0..r.count()?).map(|_| r.string()).collect::<Result<_>>()?,
                underline_sprite: r.string()?,
                link_colors: [r.u32()?, r.u32()?, r.u32()?],
                indent: r.i32()?,
                width: r.i32()?,
                fonts: refs(r)?,
                sheets: refs(r)?,
                colors: (0..r.count()?).map(|_| r.u32()).collect::<Result<_>>()?,
                images: r.external_ref()?,
            }))
        })?,
        _ => return Err(Error::Unsupported(format!("element class {class}"))),
    };
    Ok(Element { class, base, kind })
}

pub const ELEMENT_CLASSES: &[&str] = &[
    "GroupDescription",
    "PageDescription",
    "RadioDescription",
    "RibbonDescription",
    "BorderDescription",
    "ImageDescription",
    "AnimatedImageDescription",
    "FrustumOutlineDescription",
    "ButtonDescription",
    "CheckDescription",
    "ComboDescription",
    "TextDescription",
    "LabelDescription",
    "ClipRegionDescription",
    "ContainerDescription",
    "EditDescription",
    "GraphDescription",
    "LayoutDescription",
    "ListDescription",
    "NumberEditDescription",
    "NumberDescription",
    "SliderDescription",
    "SpinnerDescription",
    "TabDescription",
    "ProgressDescription",
    "ScrollDescription",
    "TableDescription",
    "DocumentTextDescription",
];

/// Decode a resource if it is any element description.
pub fn element(data: &[u8]) -> Result<Element> {
    let (_, element) = crs::parse(data, |r, h| {
        let class = ELEMENT_CLASSES
            .iter()
            .find(|c| type_id(c) == h.type_id)
            .ok_or_else(|| {
                Error::Format(format!("type id {:#010x} is not an element", h.type_id))
            })?;
        read_element(class, r)
    })?;
    Ok(element)
}

pub fn is_element(type_id_value: u32) -> bool {
    ELEMENT_CLASSES.iter().any(|c| type_id(c) == type_id_value)
}

// ---------------------------------------------------------------- Dialog

#[derive(Debug, Clone)]
pub struct DialogBinding {
    /// Application controller type ID (CRC32 of its class name).
    pub unknown: u32,
    /// Optional element description created beneath the named host.
    pub source: Ref,
    /// Optional controller configuration resource.
    pub target: Ref,
    /// Host element path identifier, or empty for the dialog root.
    pub name: String,
}

/// `UI::Dialog` (0x180115950).
#[derive(Debug, Clone)]
pub struct Dialog {
    pub unknown_40: u32,
    pub root: Ref,
    pub background_sheet: Ref,
    pub background_sprite: String,
    pub unknown_30: u32,
    pub named: Vec<(Ref, String)>,
    pub children: Vec<Ref>,
    pub bindings: Vec<DialogBinding>,
    /// Associated resources, including nested dialogs and string tables.
    pub dialogs: Vec<Ref>,
    pub unknown_34: i32,
}

pub fn dialog(data: &[u8]) -> Result<Dialog> {
    expect(data, "Dialog", |r| {
        r.block(|r, _| {
            let unknown_40 = r.u32()?;
            let root = r.external_ref()?;
            let background_sheet = r.external_ref()?;
            let background_sprite = r.string()?;
            let unknown_30 = r.u32()?;
            let named = (0..r.count()?)
                .map(|_| Ok((r.external_ref()?, r.string()?)))
                .collect::<Result<_>>()?;
            let children = refs(r)?;
            let bindings = (0..r.count()?)
                .map(|_| {
                    Ok(DialogBinding {
                        unknown: r.u32()?,
                        source: r.external_ref()?,
                        target: r.external_ref()?,
                        name: r.string()?,
                    })
                })
                .collect::<Result<_>>()?;
            let dialogs = refs(r)?;
            Ok(Dialog {
                unknown_40,
                root,
                background_sheet,
                background_sprite,
                unknown_30,
                named,
                children,
                bindings,
                dialogs,
                unknown_34: r.i32()?,
            })
        })
    })
}

// ---------------------------------------------------------------- SpriteSheet, Font, StringTable

#[derive(Debug, Clone)]
pub struct Sprite {
    pub name: String,
    pub source: String,
    /// Offset of the trimmed image inside the original source image.
    pub offset: [i32; 2],
    pub width: i32,
    pub height: i32,
    pub unknown: i32,
    /// u0, v0, u1, v1 in the sheet image.
    pub uv: [f32; 4],
}

/// `UI::SpriteSheet` (0x18011ffc0).
#[derive(Debug, Clone)]
pub struct SpriteSheet {
    pub material_path: String,
    pub source_image: String,
    pub width: i32,
    pub height: i32,
    pub unknown_flag: bool,
    pub sprites: Vec<Sprite>,
    /// Material instance whose `diffuse` texture is the sheet image.
    pub material: Ref,
}

impl SpriteSheet {
    pub fn sprite(&self, name: &str) -> Option<&Sprite> {
        self.sprites.iter().find(|s| s.name == name)
    }
}

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

/// `UI::Cursor` (`Serialize` 0x1800cf980, version 1): the sprite sheet whose image is the
/// pointer, and its hotspot.
#[derive(Debug, Clone)]
pub struct Cursor {
    pub sprite_sheet: Ref,
    pub hotspot: [i32; 2],
}

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

/// `UI::Font` (0x180119f60), block version 2.
#[derive(Debug, Clone)]
pub struct Font {
    pub name: String,
    pub size: i32,
    pub sheet_width: i32,
    pub sheet_height: i32,
    pub character_set: Ref,
    /// Sprite sheet holding one sprite per glyph, in glyph order.
    pub sprite_sheet: Ref,
    pub material: Ref,
    /// Code point -> glyph index; negative or 0xffff means no glyph.
    pub char_map: Vec<i32>,
    /// Per glyph: (a, c, b, d): advance = a + b, with the previous glyph's c added;
    /// d raises the bottom of the sprite above the baseline.
    pub glyphs: Vec<[i32; 4]>,
    /// Serialized descent, ellipsis advance, and space advance (Runtime Font +0x54/58/5c).
    pub unknown: [i32; 3],
}

impl Font {
    /// Baseline relative to a text element's top, as used by TextElement::Draw (0x180121b10).
    pub fn baseline_offset(&self) -> i32 {
        self.size - self.unknown[0]
    }

    pub fn glyph_index(&self, ch: char) -> Option<usize> {
        let g = *self.char_map.get(ch as usize)?;
        (g >= 0 && g != 0xFFFF && (g as usize) < self.glyphs.len()).then_some(g as usize)
    }
}

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

/// `UI::StringTable` (0x1801201d0): key -> display string.
pub fn string_table(data: &[u8]) -> Result<Vec<(String, String)>> {
    expect(data, "StringTable", |r| {
        r.block(|r, _| {
            (0..r.count()?)
                .map(|_| Ok((r.string()?, r.string()?)))
                .collect()
        })
    })
}

fn expect<T>(data: &[u8], class: &str, read: impl FnOnce(&mut Reader) -> Result<T>) -> Result<T> {
    let (_, value) = crs::parse(data, |r, h| {
        if h.type_id != type_id(class) {
            return format_err(format!(
                "expected {class}, found type id {:#010x}",
                h.type_id
            ));
        }
        read(r)
    })?;
    Ok(value)
}
