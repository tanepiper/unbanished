#![doc = include_str!("../README.md")]
#![doc(
    html_logo_url = "https://tanepiper.github.io/unbanished/assets/un-banished-icon.svg",
    html_favicon_url = "https://tanepiper.github.io/unbanished/favicon.svg"
)]

mod animation;
mod controls;
mod dialog;
mod document;
mod draw;
mod input;
mod instantiate;
mod layout;
mod query;
mod resources;
mod text;
mod tree;

use draw::modulate;

use std::rc::Rc;

use banished_formats::pkg::Archive;
use banished_formats::ui::{flags, Element, ElementKind};

pub use controls::{Control, EditData, SliderData};
pub use dialog::{Binding, DialogInstance};
pub use input::{tooltip_controller, Key};
pub use query::{ElementInfo, Query, Step};
pub use resources::{FontId, LoadedFont, LoadedSheet, Resources, SheetId, SpriteRef};
pub use text::TextLayout;

/// The index of an element in [`Ui::nodes`].
pub type NodeId = usize;

/// What a quad samples from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Texture {
    /// A sprite sheet of the game, loaded by [`Resources`].
    Sheet(SheetId),
    /// Application-supplied texture (for example a texture preview).
    External(u32),
}

/// One textured rectangle to draw. [`Ui::draw`] returns them in drawing order.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quad {
    /// What the quad samples from.
    pub texture: Texture,
    /// The rectangle in pixels: x0, y0, x1, y1.
    pub rect: [f32; 4],
    /// The texture coordinates: u0, v0, u1, v1.
    pub uv: [f32; 4],
    /// The colour that multiplies the texture: red, green, blue, alpha.
    pub color: [u8; 4],
    /// Scissor rectangle (x0, y0, x1, y1) in pixels.
    pub clip: Option<[i32; 4]>,
    /// Degrees about the rectangle's centre, clockwise on screen (`ImageElement::SetRotation`).
    pub rotation: f32,
}

/// The text of a text element or a label, with its font and the result of its layout.
#[derive(Debug, Clone)]
pub struct TextData {
    /// The font, or `None` when it could not be loaded.
    pub font: Option<FontId>,
    /// The text.
    pub text: String,
    /// The width at which the text wraps, in pixels (0: no limit).
    pub max_width: i32,
    /// The justification of the lines (left, centre or right).
    pub justify: u32,
    /// Whether long lines wrap.
    pub wrap: bool,
    /// Whether the text has a shadow.
    pub shadow: bool,
    /// The width below which the text ends with an ellipsis (0: never).
    pub ellipsis_width: i32,
    /// The laid-out lines and glyphs.
    pub layout: TextLayout,
}

/// The rows of a list.
#[derive(Debug, Clone)]
pub struct ListData {
    /// The text of each row.
    pub items: Vec<String>,
    /// Each item's sprite (`ComboElement::AddItem`'s `Sprite`), drawn before its text.
    pub icons: Vec<Option<SpriteRef>>,
    /// The selected row.
    pub selected: Option<usize>,
    /// The row under the pointer.
    pub hovered: Option<usize>,
    /// First visible pixel row.
    pub scroll: i32,
    /// The font of the rows.
    pub font: Option<FontId>,
    /// The height of a row, in pixels.
    pub row_height: i32,
    /// The sprite drawn behind the selected row.
    pub highlight: Option<SpriteRef>,
    rows: Vec<TextLayout>,
    /// Scroll the selection into view at the next layout.
    reveal: bool,
}

/// What an element is, with the state of its class.
#[derive(Debug, Clone)]
pub enum Kind {
    /// A group: its children are laid out on top of each other, each by its alignment.
    Group,
    /// A ribbon: its children in a row or a column.
    Ribbon {
        /// A column when true, a row when false.
        vertical: bool,
        /// Whether every cell has the size of the largest child.
        uniform: bool,
        /// The space between the cells, in pixels.
        spacing: i32,
        /// The size of each cell after the last layout.
        cells: Vec<i32>,
    },
    /// A frame drawn from nine sprites: four corners, four edges and the centre.
    Border {
        /// The nine sprites, row by row.
        sprites: [Option<SpriteRef>; 9],
    },
    /// One sprite.
    Image {
        /// The sprite.
        sprite: Option<SpriteRef>,
        /// Its size in pixels.
        size: [i32; 2],
    },
    /// A sequence of sprites played as an animation (for example the loading screen).
    AnimatedImage {
        /// The frames.
        sprites: Vec<Option<SpriteRef>>,
        /// The size in pixels.
        size: [i32; 2],
        /// The frame rate.
        frames_per_second: i32,
        /// The frame shown now.
        frame: usize,
        // +0xf8
        /// Its own clock, from its creation, kept within one loop of the frames.
        clock: f64,
    },
    /// A button: one child element for each state, and its content.
    Button {
        /// The element shown in each state: normal, hot (under the pointer), pressed, disabled.
        states: [Option<NodeId>; 4],
        /// The content (a label or an image).
        content: Option<NodeId>,
        /// The content's colour in each state.
        colors: [u32; 4],
        /// The current state, 0 to 3.
        state: usize,
    },
    /// Text.
    Text(TextData),
    /// Text with an optional icon.
    Label {
        /// The text.
        text: TextData,
        /// The icon.
        icon: Option<SpriteRef>,
        /// The icon's size in pixels.
        icon_size: [i32; 2],
        /// The alignment of the text.
        text_alignment: u32,
        /// Whether the icon is beside the text or above it.
        orientation: u32,
        /// The smallest width of the text, in pixels.
        min_text_width: i32,
        /// The space between the icon and the text, in pixels.
        gap: i32,
        // +0x128
        /// The icon's colour (`LabelElement::SetSpriteColor`), drawn as it is.
        icon_color: u32,
    },
    /// Reserves space the application draws into (3D view, previews).
    Viewport,
    /// Scrolling list of text rows (tool widget, styled with game assets).
    List(ListData),
    /// Line segments in element pixels, drawn with the sprite in the element colour and clipped
    /// to the element (the game's `FrustumOutlineElement`, `SpriteContext::AddLine`).
    Lines {
        /// The sprite the lines are drawn with.
        sprite: Option<SpriteRef>,
        /// The element's size in pixels.
        size: [i32; 2],
        /// The lines: x0, y0, x1, y1 in element pixels.
        lines: Vec<[i32; 4]>,
    },
    /// Application texture drawn letterboxed inside the element.
    Picture {
        /// The application's texture number ([`Texture::External`]).
        texture: u32,
        /// The texture's size in pixels.
        size: [i32; 2],
    },
    /// A control with state: a check box, a slider, an edit box, a combo box and others.
    Control(Control),
    /// An element class that is not implemented, by name. It is laid out but not drawn.
    Unsupported(&'static str),
}

/// One element of the interface tree.
#[derive(Debug, Clone)]
pub struct Node {
    /// The element's name in its resource, for example `buttonNew`. The game finds elements by
    /// it.
    pub name: String,
    /// The reference of the resource the element was made from.
    pub reference: String,
    /// The element's border, drawn behind it.
    pub border: Option<NodeId>,
    /// The key that clicks the element.
    pub shortcut: Option<u32>,
    /// The margins: left, right, top, bottom.
    pub margins: [i32; 4],
    /// The smallest size, in pixels.
    pub min_size: [i32; 2],
    /// How the element sits in the space it is given ([`flags`] has the bits).
    pub alignment: u32,
    /// The element's flags ([`flags`]).
    pub flags: u32,
    /// The colour that multiplies the element, `0xAARRGGBB`.
    pub color: u32,
    /// What the element is.
    pub kind: Kind,
    /// The children, in drawing order.
    pub children: Vec<NodeId>,
    /// The tooltip text.
    pub tooltip: String,
    /// The dialog shown as the tooltip, if the tooltip is a dialog.
    pub tooltip_dialog: Option<String>,
    // +0x48
    /// Element: the string table the element's own strings come from.
    pub string_table: Option<String>,
    // Element +0xc0
    /// Name of the element's key binding (`Element` resolved through `keyNames`), for
    /// `HotKeyToolTipUI`.
    pub hotkey: Option<String>,
    /// Title and description user data, for `TitleDescToolTipUI`.
    pub title_desc: Option<(String, String)>,
    /// Whether the element takes input.
    pub enabled: bool,
    // +0x3c
    /// Element: the mouse cursor index over it (`Element::GetMouseCursorIndex`).
    pub cursor: i32,
    /// An image's rotation in degrees (`ImageElement::SetRotation`).
    pub rotation: f32,
    /// Content size from the last size pass.
    pub size: [i32; 2],
    /// Layout rectangle: left, top, right, bottom.
    pub rect: [i32; 4],
    /// The layout pass whose `size` this is (`Ui::size_pass`): a node is measured once per
    /// pass, however many of its ancestors ask for its size.
    sized_pass: u32,
}

impl Node {
    /// An element of this kind, with the default values.
    pub fn new(kind: Kind) -> Node {
        Node {
            name: String::new(),
            reference: String::new(),
            border: None,
            shortcut: None,
            margins: [0; 4],
            min_size: [0; 2],
            alignment: 0,
            flags: 0,
            color: 0xFFFF_FFFF,
            kind,
            children: Vec::new(),
            tooltip: String::new(),
            tooltip_dialog: None,
            string_table: None,
            hotkey: None,
            title_desc: None,
            enabled: true,
            cursor: 0,
            rotation: 0.0,
            size: [0; 2],
            rect: [0; 4],
            sized_pass: 0,
        }
    }

    /// Whether the element is shown.
    pub fn visible(&self) -> bool {
        self.flags & flags::HIDDEN == 0
    }

    // +0x34 & 0x1800000
    /// Enabled and not locked by the tutorial (`& 0x1800000` clear).
    pub fn usable(&self) -> bool {
        self.enabled && self.flags & flags::LOCKED == 0
    }

    /// Whether a point is inside the element's rectangle.
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.rect[0] && x < self.rect[2] && y >= self.rect[1] && y < self.rect[3]
    }
}

/// Something the player did to an element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// A button was clicked.
    Clicked(NodeId),
    /// A row of a list or a combo box was selected.
    Selected(NodeId, usize),
    /// A check box, a slider or an edit box changed.
    Changed(NodeId),
    /// Enter was pressed in an edit box.
    Submitted(NodeId),
    /// A link in a document was clicked (its index).
    Link(NodeId, usize),
}

/// The interface: the element tree, the resources it uses, and the state of the pointer and the
/// keyboard.
pub struct Ui {
    /// The sprite sheets, fonts, element descriptions and strings, loaded once.
    pub res: Resources,
    /// Every element. A [`NodeId`] is an index into this list.
    pub nodes: Vec<Node>,
    /// The screen's size in pixels.
    pub screen: [i32; 2],
    pointer: (i32, i32),
    pointer_down: bool,
    hot: Option<NodeId>,
    pressed: Option<NodeId>,
    focus: Option<NodeId>,
    input_root: Option<NodeId>,
    /// The current layout pass, for [`Node::sized_pass`].
    size_pass: u32,
    tooltip_instance: Option<(NodeId, NodeId)>,
    /// `UserInterface::Play` requests (sound slots: 4 button down, 5 button up/click, 3 drop),
    /// drained by the application.
    pub sounds: Vec<usize>,
    /// A tooltip dialog opened since the application last asked (`new_tooltip`), with its
    /// controller ids, for application-defined tooltip controllers.
    tooltip_opened: Option<(NodeId, NodeId, Vec<u32>)>,
    pending: Vec<Event>,
    stack: Vec<String>,
    overrides: Vec<std::collections::HashMap<String, Option<String>>>,
}

impl Ui {
    /// An empty interface that reads its resources from `archive`.
    pub fn new(archive: Rc<Archive>) -> Ui {
        Ui {
            res: Resources::new(archive),
            nodes: Vec::new(),
            screen: [1280, 720],
            pointer: (-1, -1),
            pointer_down: false,
            hot: None,
            pressed: None,
            focus: None,
            input_root: None,
            size_pass: 1,
            tooltip_instance: None,
            tooltip_opened: None,
            sounds: Vec::new(),
            pending: Vec::new(),
            stack: Vec::new(),
            overrides: Vec::new(),
        }
    }

    /// Add an element, and return its id.
    pub fn add(&mut self, mut node: Node) -> NodeId {
        node.sized_pass = 0;
        self.nodes.push(node);
        self.nodes.len() - 1
    }

    /// Make `child` the last child of `parent`.
    pub fn add_child(&mut self, parent: NodeId, child: NodeId) {
        self.nodes[parent].children.push(child);
    }

    /// An element.
    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id]
    }

    /// An element, to change it.
    pub fn node_mut(&mut self, id: NodeId) -> &mut Node {
        &mut self.nodes[id]
    }

    // ------------------------------------------------------------ instantiation

    // ------------------------------------------------------------ tool widgets

    // ------------------------------------------------------------ layout

    // ------------------------------------------------------------ drawing
}
