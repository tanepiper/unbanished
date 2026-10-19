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

/// The bits of an element's flags.
pub mod flags {
    /// The element is hidden.
    pub const HIDDEN: u32 = 1 << 0;
    /// The element fills the width it is given.
    pub const FILL_WIDTH: u32 = 1 << 2;
    /// The element fills the height it is given.
    pub const FILL_HEIGHT: u32 = 1 << 3;
    /// Height (0x10) / width (0x20) taken from the screen size times the UI scale.
    pub const SCREEN_HEIGHT: u32 = 1 << 4;
    /// The width is the screen's width times the interface scale.
    pub const SCREEN_WIDTH: u32 = 1 << 5;
    /// Halves the element's alpha (`Element::ComputeColor`).
    pub const DIM: u32 = 1 << 18;
    /// Takes the mouse (`SetFlagsOnOff(0x10000)`): a click on it is an event of its own.
    pub const MOUSE: u32 = 1 << 16;
    /// Disabled by the tutorial (input passes it by, like a disabled one).
    ///
    /// Original: `FUN_1400e7720`.
    pub const LOCKED: u32 = 1 << 24;
}

/// Shared by every element.
///
/// Original: `UI::ElementDescription`, Runtime `0x180119390`.
#[derive(Debug, Clone, Default)]
pub struct ElementBase {
    /// left, right, top, bottom
    pub margins: [i32; 4],
    /// The smallest size, in pixels.
    pub min_size: [i32; 2],
    /// How the element sits in the space it is given.
    pub alignment: Alignment,
    /// The element's flags ([`flags`]).
    pub flags: u32,
    /// ARGB, multiplied into everything the element draws.
    pub color: u32,
    // +0x3c
    /// The mouse cursor shown over it (`Element::GetMouseCursorIndex`; an index into the game's
    /// cursors: 0 Arrow, 1 Hand, 2 Hourglass, 3 Move, 4 Query, 5 Place). Only the StandardDialog
    /// title bar sets one (3).
    pub cursor: i32,
    /// The element drawn behind it as a border.
    pub border: Ref,
    /// The dialog shown as the tooltip, when the tooltip is a dialog.
    pub tooltip_dialog: Ref,
    /// The string table the element's own strings come from.
    pub string_table: Ref,
    /// The tooltip text, or its key in the string table.
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

/// A button's description.
#[derive(Debug, Clone)]
pub struct Button {
    /// enabled, disabled, highlight, pressed state elements
    pub states: [Ref; 4],
    /// The content: a label or an image.
    pub content: Ref,
    /// Content colours per state (ARGB), same order as `states`.
    pub colors: [u32; 4],
}

/// A text element's description.
#[derive(Debug, Clone)]
pub struct Text {
    /// The font.
    pub font: Ref,
    /// The string table that `text` is a key of.
    pub string_table: Ref,
    /// String-table key, or literal text when no table is given.
    pub text: String,
    /// Draw a black shadow one pixel down and right (TextElement::Draw).
    pub shadow: bool,
    /// Whether long lines wrap.
    pub wrap: bool,
    /// Width limit passed to `Font::ComputeStringLayout` (wrapping / ellipsis).
    pub max_width: i32,
    /// `Font::Justify`: 0 left, 1 centre, 2 right.
    pub justify: u32,
    /// Non-wrapped line width at which the engine inserts "...". This is not a key code.
    pub ellipsis_width: i32,
}

/// A label's description: text with an optional icon.
#[derive(Debug, Clone)]
pub struct Label {
    /// The text.
    pub text: Text,
    /// The icon's sprite sheet.
    pub sprite_sheet: Ref,
    /// The icon's sprite name.
    pub sprite: String,
    /// The alignment of the text.
    pub text_alignment: u32,
    /// > 1 stacks icon above text, otherwise icon left of text (`LabelElement::ComputeSize`).
    pub orientation: u32,
    /// The smallest width of the text, in pixels.
    pub min_text_width: i32,
    /// The space between the icon and the text, in pixels.
    pub gap: i32,
    /// The icon's size, in pixels.
    pub icon_size: [i32; 2],
}

/// What an element description is, with the fields of its class.
#[derive(Debug, Clone)]
pub enum ElementKind {
    /// Children laid out on top of each other, each by its alignment.
    Group {
        /// The children.
        children: Vec<Ref>,
    },
    /// Pages, of which one is shown.
    Page {
        /// The pages.
        children: Vec<Ref>,
        /// The page shown at first.
        page: i32,
    },
    /// Radio buttons, of which one is selected.
    Radio {
        /// The buttons.
        children: Vec<Ref>,
        /// The button selected at first.
        selected: i32,
    },
    /// Children in a row or a column.
    Ribbon {
        /// A column when true, a row when false.
        vertical: bool,
        /// Whether every cell has the size of the largest child.
        uniform: bool,
        /// The space between the cells, in pixels.
        spacing: i32,
        /// The children.
        children: Vec<Ref>,
    },
    /// A frame drawn from nine sprites.
    Border {
        /// The sprite sheet.
        sprite_sheet: Ref,
        /// The nine sprites, row by row: corners, edges and centre.
        sprites: [String; 9],
    },
    /// One sprite.
    Image {
        /// The sprite sheet.
        sprite_sheet: Ref,
        /// The sprite name.
        sprite: String,
        /// The size, in pixels.
        size: [i32; 2],
    },
    /// The game's `FrustumOutlineDescription` (MapUI's camera outline): an image block whose
    /// element draws lines instead of the sprite.
    ///
    /// Original: `FUN_14006c7f0`.
    FrustumOutline {
        /// The sprite sheet.
        sprite_sheet: Ref,
        /// The sprite the lines are drawn with.
        sprite: String,
        /// The size, in pixels.
        size: [i32; 2],
    },
    /// A sequence of sprites played as an animation.
    AnimatedImage {
        /// The sprite sheet.
        sprite_sheet: Ref,
        /// The first frame's sprite.
        sprite: String,
        /// The size, in pixels.
        size: [i32; 2],
        /// The frames' sprite names.
        frames: Vec<String>,
        /// The frame rate.
        rate: i32,
    },
    /// A button.
    Button(Button),
    /// A check box.
    Check {
        /// The box.
        button: Button,
        /// The tick, shown when checked.
        check: Ref,
    },
    /// A combo box: a button that opens a list.
    Combo {
        /// The button that shows the choice.
        button: Button,
        /// The list's parts: the second is the text whose font the list uses, the third the
        /// choices.
        refs: [Ref; 3],
    },
    /// Text.
    Text(Text),
    /// Text with an optional icon.
    Label(Label),
    /// A window onto its content, cut at its edges.
    ClipRegion {
        /// The window's size, in pixels.
        unknown: [i32; 2],
        /// The content.
        content: Ref,
    },
    /// A scrolling area.
    Container {
        /// The size (two values) and the space between the content and the bars.
        unknown: [i32; 3],
        /// The horizontal bar, the vertical bar and the content.
        refs: [Ref; 3],
        /// Not identified yet.
        unknown_bools: [bool; 2],
    },
    /// An edit box.
    Edit {
        /// The font.
        font: Ref,
        /// The text at first.
        text: String,
        /// The most characters it takes (0: no limit).
        max_length: i32,
        /// Whether the text is drawn as `*`s (a password).
        unknown_bool: bool,
        /// The characters it takes, any when empty.
        unknown_string: String,
    },
    /// A graph of values over time.
    Graph {
        /// The size (two values) and the length of the ticks.
        unknown: [i32; 3],
        /// Not identified yet.
        unknown_u32: u32,
        /// The sprite sheet of the lines.
        sprite_sheet: Ref,
        /// The sprite the lines are drawn with.
        sprite: String,
    },
    /// Children placed by anchors relative to the element.
    Layout {
        /// The children and their anchors.
        items: Vec<LayoutItem>,
    },
    /// A list of rows.
    List {
        /// The list's parts: the first is the text whose font the rows use.
        refs: [Ref; 2],
        /// The sprite sheet of the highlight.
        sprite_sheet: Ref,
        /// The highlight's sprite.
        sprite: String,
        /// Not identified yet.
        unknown: [u32; 2],
    },
    /// An edit box for a number.
    NumberEdit {
        /// The font.
        font: Ref,
        /// The value at first, the smallest and largest value, and the most digits.
        unknown: [i32; 4],
        /// Whether the number may be below zero.
        unknown_bool: bool,
    },
    /// A number shown as text.
    Number {
        /// The font.
        font: Ref,
        /// The colour, `0xAARRGGBB`.
        color: u32,
        /// Not identified yet.
        unknown_int: i32,
        /// The value at first.
        unknown_float: f32,
        /// Not identified yet.
        unknown_bool: bool,
    },
    /// A slider.
    Slider {
        /// The number shown next to the slider.
        track: Ref,
        /// The sprite sheet.
        sprite_sheet: Ref,
        /// The sprites: the track's ends and middle, the knob, and the knob under the pointer.
        sprites: [String; 5],
        /// The track's vertical position and the inset of the knob's travel.
        unknown: [i32; 2],
        /// The smallest and the largest value.
        range: [f32; 2],
        /// The steps across the range: a click on the track moves the value one step.
        unknown_b0: i32,
    },
    /// A number with up and down arrows.
    Spinner {
        /// The number and the two arrows.
        refs: [Ref; 3],
        /// How much an arrow changes the number.
        unknown: i32,
    },
    /// Tabs: a header for each tab, and the page of the selected one.
    Tab {
        /// The elements drawn around the tabs.
        refs: [Ref; 3],
        /// The sprite sheet.
        sprite_sheet: Ref,
        /// The sprites of the tab headers.
        sprites: [String; 3],
        /// The space between the headers, in pixels.
        unknown: i32,
        /// Each tab: its header and its page.
        tabs: Vec<(Ref, Ref)>,
    },
    /// A progress bar.
    Progress {
        /// The background's colour, `0xAARRGGBB`.
        color: u32,
        /// The sprite sheet.
        sprite_sheet: Ref,
        /// The sprites of the fill and the background.
        sprites: [String; 6],
        /// The element drawn on top (usually a label).
        unknown_ref: Ref,
    },
    /// A scroll bar.
    Scroll {
        /// The sprite sheet.
        sprite_sheet: Ref,
        /// The sprites of the track, the thumb and the arrows.
        sprites: [String; 9],
        /// A vertical bar when true.
        vertical: bool,
        /// The thumb's inset at both ends of the track.
        unknown: i32,
        /// The thumb's colour, and a value not identified yet.
        unknown_u32: [u32; 2],
    },
    /// A table of cells in rows and columns.
    Table {
        /// The number of columns, the number of rows, and the space between the cells.
        unknown: [i32; 3],
        /// The cells.
        cells: Vec<TableCell>,
    },
    /// A document with markup (the help pages and the tutorials).
    DocumentText(DocumentText),
}

/// Rich text configuration (`DocumentTextDescription::Serialize`).
///
/// Original: Runtime `0x1800d2110`.
#[derive(Debug, Clone)]
pub struct DocumentText {
    /// The string table that `text` is a key of.
    pub string_table: Ref,
    /// The text with markup, or its key.
    pub text: String,
    /// The sprite sheet of the bullets and the underline.
    pub sprite_sheet: Ref,
    /// The bullets' sprite names.
    pub bullets: Vec<String>,
    /// The underline's sprite name.
    pub underline_sprite: String,
    /// The colours of links: normal, under the pointer, pressed.
    pub link_colors: [u32; 3],
    /// The indent of list items, in pixels.
    pub indent: i32,
    /// The width at which the text wraps, in pixels.
    pub width: i32,
    /// The fonts the markup can select.
    pub fonts: Vec<Ref>,
    /// The sprite sheets the markup can show images from.
    pub sheets: Vec<Ref>,
    /// The colours the markup can select.
    pub colors: Vec<u32>,
    /// The resource with the document's pictures.
    pub images: Ref,
}

/// A child of a layout element, with its anchors.
#[derive(Debug, Clone)]
pub struct LayoutItem {
    /// The child.
    pub element: Ref,
    /// The names of the anchors.
    pub strings: [String; 3],
    /// Not identified yet.
    pub unknown_u32: [u32; 2],
    /// The offsets from the anchors, in pixels.
    pub unknown_i32: [i32; 2],
    /// Not identified yet.
    pub flag: bool,
}

/// A cell of a table element.
#[derive(Debug, Clone)]
pub struct TableCell {
    /// The column.
    pub column: i32,
    /// The row.
    pub row: i32,
    /// The cell's element.
    pub element: Ref,
}

/// An element description: its class, the fields every element has, and its class's fields.
#[derive(Debug, Clone)]
pub struct Element {
    /// The element's class name, for example `ButtonDescription`.
    pub class: &'static str,
    /// The fields every element has.
    pub base: ElementBase,
    /// The class and its own fields.
    pub kind: ElementKind,
}

impl Element {
    /// The references of its child elements.
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

/// The class names of every element description.
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

/// Whether a type id is one of the element classes.
pub fn is_element(type_id_value: u32) -> bool {
    ELEMENT_CLASSES.iter().any(|c| type_id(c) == type_id_value)
}

// ---------------------------------------------------------------- Dialog

/// A controller attached to a dialog's element.
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

/// Original: `UI::Dialog`, Runtime `0x180115950`.
#[derive(Debug, Clone)]
pub struct Dialog {
    /// Not identified yet.
    pub unknown_40: u32,
    /// The root element.
    pub root: Ref,
    /// The sprite sheet of the background drawn behind the dialog.
    pub background_sheet: Ref,
    /// The background's sprite name.
    pub background_sprite: String,
    /// Not identified yet.
    pub unknown_30: u32,
    /// Elements replaced by name: the replacement resource and the name of the element it replaces.
    pub named: Vec<(Ref, String)>,
    /// The dialog's elements.
    pub children: Vec<Ref>,
    /// The controllers attached to elements.
    pub bindings: Vec<DialogBinding>,
    /// Associated resources, including nested dialogs and string tables.
    pub dialogs: Vec<Ref>,
    /// Not identified yet.
    pub unknown_34: i32,
}

/// Read a [`Dialog`] from the bytes of its resource.
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

impl SpriteSheet {
    /// A sprite by name.
    pub fn sprite(&self, name: &str) -> Option<&Sprite> {
        self.sprites.iter().find(|s| s.name == name)
    }
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
