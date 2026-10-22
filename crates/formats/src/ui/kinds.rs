//! The element classes' own data: buttons, text, labels, documents, layouts and table cells.

use super::*;

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
