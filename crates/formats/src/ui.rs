//! `UI::` resources: dialogs, element descriptions, fonts, sprite sheets and string tables.
//!
//! Every class level of `Serialize` is its own block and derived classes serialize their base first.
//! Field meanings come from `ElementDescription` defaults, `Element::Element` and the per-class
//! `ComputeSize`/`ComputeLayout` code; names marked "unknown" have not been traced to behaviour yet.

mod kinds;
mod read;
pub use kinds::{Button, DocumentText, ElementKind, Label, LayoutItem, TableCell, Text};
use read::read_element;

mod dialogs;
mod fonts;
mod sprites;
pub use dialogs::{dialog, Dialog, DialogBinding};
pub use fonts::{font, string_table, Font};
pub use sprites::{cursor, sprite_sheet, Cursor, Sprite, SpriteSheet};

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

// ---------------------------------------------------------------- SpriteSheet, Font, StringTable

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
