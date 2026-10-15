//! Reimplementation of the Banished UI system (`UI::Element` and friends).
//!
//! Elements are instantiated from the game's `*Description` resources (or built in code from them),
//! sized bottom-up and laid out top-down following `ComputeSize`/`ComputeLayout` in
//! Runtime-steam-x64.dll, and drawn as textured quads that a renderer batches. Behaviour notes and
//! addresses are in `docs/FINDINGS.md`.

mod controls;
mod dialog;
mod document;
mod input;
mod resources;
mod text;

use std::rc::Rc;

use banished_formats::pkg::Archive;
use banished_formats::ui::{flags, Element, ElementKind};

pub use controls::{Control, EditData, SliderData};
pub use dialog::{Binding, DialogInstance};
pub use input::{tooltip_controller, Key};
pub use resources::{FontId, LoadedFont, LoadedSheet, Resources, SheetId, SpriteRef};
pub use text::TextLayout;

pub type NodeId = usize;

/// What a quad samples from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Texture {
    Sheet(SheetId),
    /// Application-supplied texture (for example a texture preview).
    External(u32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quad {
    pub texture: Texture,
    /// x0, y0, x1, y1 in pixels.
    pub rect: [f32; 4],
    pub uv: [f32; 4],
    /// RGBA
    pub color: [u8; 4],
    /// Scissor rectangle (x0, y0, x1, y1) in pixels.
    pub clip: Option<[i32; 4]>,
}

#[derive(Debug, Clone)]
pub struct TextData {
    pub font: Option<FontId>,
    pub text: String,
    pub max_width: i32,
    pub justify: u32,
    pub wrap: bool,
    pub shadow: bool,
    pub ellipsis_width: i32,
    pub layout: TextLayout,
}

#[derive(Debug, Clone)]
pub struct ListData {
    pub items: Vec<String>,
    pub selected: Option<usize>,
    pub hovered: Option<usize>,
    /// First visible pixel row.
    pub scroll: i32,
    pub font: Option<FontId>,
    pub row_height: i32,
    pub highlight: Option<SpriteRef>,
    rows: Vec<TextLayout>,
    /// Scroll the selection into view at the next layout.
    reveal: bool,
}

#[derive(Debug, Clone)]
pub enum Kind {
    Group,
    Ribbon {
        vertical: bool,
        uniform: bool,
        spacing: i32,
        cells: Vec<i32>,
    },
    Border {
        sprites: [Option<SpriteRef>; 9],
    },
    Image {
        sprite: Option<SpriteRef>,
        size: [i32; 2],
    },
    AnimatedImage {
        sprites: Vec<Option<SpriteRef>>,
        size: [i32; 2],
        frames_per_second: i32,
        frame: usize,
    },
    Button {
        states: [Option<NodeId>; 4],
        content: Option<NodeId>,
        colors: [u32; 4],
        state: usize,
    },
    Text(TextData),
    Label {
        text: TextData,
        icon: Option<SpriteRef>,
        icon_size: [i32; 2],
        text_alignment: u32,
        orientation: u32,
        min_text_width: i32,
        gap: i32,
    },
    /// Reserves space the application draws into (3D view, previews).
    Viewport,
    /// Scrolling list of text rows (tool widget, styled with game assets).
    List(ListData),
    /// Line segments in element pixels, drawn with the sprite in the element colour and clipped
    /// to the element (the game's `FrustumOutlineElement`, `SpriteContext::AddLine`).
    Lines {
        sprite: Option<SpriteRef>,
        size: [i32; 2],
        lines: Vec<[i32; 4]>,
    },
    /// Application texture drawn letterboxed inside the element.
    Picture {
        texture: u32,
        size: [i32; 2],
    },
    Control(Control),
    Unsupported(&'static str),
}

#[derive(Debug, Clone)]
pub struct Node {
    pub name: String,
    pub reference: String,
    pub border: Option<NodeId>,
    pub shortcut: Option<u32>,
    /// left, right, top, bottom
    pub margins: [i32; 4],
    pub min_size: [i32; 2],
    pub alignment: u32,
    pub flags: u32,
    pub color: u32,
    pub kind: Kind,
    pub children: Vec<NodeId>,
    pub tooltip: String,
    pub tooltip_dialog: Option<String>,
    /// Element `+0x48`: the string table the element's own strings come from.
    pub string_table: Option<String>,
    /// Name of the element's key binding (`Element +0xc0` resolved through `keyNames`), for
    /// `HotKeyToolTipUI`.
    pub hotkey: Option<String>,
    /// Title and description user data, for `TitleDescToolTipUI`.
    pub title_desc: Option<(String, String)>,
    pub enabled: bool,
    /// Content size from the last size pass.
    pub size: [i32; 2],
    /// Layout rectangle: left, top, right, bottom.
    pub rect: [i32; 4],
}

impl Node {
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
            size: [0; 2],
            rect: [0; 4],
        }
    }

    pub fn visible(&self) -> bool {
        self.flags & flags::HIDDEN == 0
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.rect[0] && x < self.rect[2] && y >= self.rect[1] && y < self.rect[3]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Clicked(NodeId),
    Selected(NodeId, usize),
    Changed(NodeId),
    Submitted(NodeId),
    Link(NodeId, usize),
}

pub struct Ui {
    pub res: Resources,
    pub nodes: Vec<Node>,
    pub screen: [i32; 2],
    pointer: (i32, i32),
    pointer_down: bool,
    hot: Option<NodeId>,
    pressed: Option<NodeId>,
    focus: Option<NodeId>,
    input_root: Option<NodeId>,
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

fn argb_to_rgba(c: u32, alpha: u8) -> [u8; 4] {
    let a = ((c >> 24) & 0xFF) * alpha as u32 / 255;
    [(c >> 16) as u8, (c >> 8) as u8, c as u8, a as u8]
}

fn modulate(a: u32, b: u32) -> u32 {
    let ch = |s: u32| ((a >> s & 0xFF) * (b >> s & 0xFF) / 255) << s;
    ch(24) | ch(16) | ch(8) | ch(0)
}

impl Ui {
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
            tooltip_instance: None,
            tooltip_opened: None,
            sounds: Vec::new(),
            pending: Vec::new(),
            stack: Vec::new(),
            overrides: Vec::new(),
        }
    }

    pub fn add(&mut self, node: Node) -> NodeId {
        self.nodes.push(node);
        self.nodes.len() - 1
    }

    pub fn add_child(&mut self, parent: NodeId, child: NodeId) {
        self.nodes[parent].children.push(child);
    }

    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id]
    }

    pub fn node_mut(&mut self, id: NodeId) -> &mut Node {
        &mut self.nodes[id]
    }

    // ------------------------------------------------------------ instantiation

    /// Build a node tree from an element reference such as `Dialog\MainMenu.rsc:main`.
    pub fn instantiate(&mut self, reference: &str) -> Option<NodeId> {
        let name = reference
            .rsplit_once(':')
            .map_or(reference, |(_, name)| name);
        let reference = self
            .overrides
            .last()
            .and_then(|o| o.get(name))
            .cloned()
            .unwrap_or_else(|| Some(reference.to_string()))?;
        if self.stack.len() >= 128 || self.stack.contains(&reference) {
            log::warn!("recursive UI reference: {reference}");
            return None;
        }
        self.stack.push(reference.clone());
        let result = self.instantiate_reference(&reference);
        self.stack.pop();
        result
    }

    fn instantiate_reference(&mut self, reference: &str) -> Option<NodeId> {
        let element = self.res.element(reference)?;
        let name = reference
            .rsplit_once(':')
            .map_or(reference, |(_, n)| n)
            .to_string();
        let id = self.instantiate_element(&element, name);
        self.nodes[id].reference = reference.to_string();
        Some(id)
    }

    fn text_data(&mut self, t: &banished_formats::ui::Text) -> TextData {
        let font = t.font.as_deref().and_then(|f| self.res.font(f));
        let text = self.res.string(t.string_table.as_deref(), &t.text);
        TextData {
            font,
            text,
            max_width: t.max_width,
            justify: t.justify,
            wrap: t.wrap,
            shadow: t.shadow,
            ellipsis_width: t.ellipsis_width,
            layout: TextLayout::default(),
        }
    }

    fn instantiate_element(&mut self, e: &Element, name: String) -> NodeId {
        let child = |ui: &mut Ui, r: &Option<String>| r.as_deref().and_then(|r| ui.instantiate(r));
        let mut children = Vec::new();
        let kind = match &e.kind {
            ElementKind::Group { children: c } => {
                children = c.iter().filter_map(|r| child(self, r)).collect();
                Kind::Group
            }
            ElementKind::Ribbon {
                vertical,
                uniform,
                spacing,
                children: c,
            } => {
                children = c.iter().filter_map(|r| child(self, r)).collect();
                Kind::Ribbon {
                    vertical: *vertical,
                    uniform: *uniform,
                    spacing: *spacing,
                    cells: Vec::new(),
                }
            }
            ElementKind::Border {
                sprite_sheet,
                sprites,
            } => {
                let mut out = [None; 9];
                for (o, s) in out.iter_mut().zip(sprites) {
                    *o = self.res.sprite(sprite_sheet.as_deref(), s);
                }
                Kind::Border { sprites: out }
            }
            ElementKind::Image {
                sprite_sheet,
                sprite,
                size,
            } => Kind::Image {
                sprite: self.res.sprite(sprite_sheet.as_deref(), sprite),
                size: *size,
            },
            ElementKind::FrustumOutline {
                sprite_sheet,
                sprite,
                size,
            } => Kind::Lines {
                sprite: self.res.sprite(sprite_sheet.as_deref(), sprite),
                size: *size,
                lines: Vec::new(),
            },
            ElementKind::AnimatedImage {
                sprite_sheet,
                sprite,
                size,
                frames,
                rate,
            } => {
                let mut sprites = if frames.is_empty() {
                    vec![self.res.sprite(sprite_sheet.as_deref(), sprite)]
                } else {
                    frames
                        .iter()
                        .map(|frame| self.res.sprite(sprite_sheet.as_deref(), frame))
                        .collect()
                };
                if sprites.is_empty() {
                    sprites.push(self.res.sprite(sprite_sheet.as_deref(), sprite));
                }
                Kind::AnimatedImage {
                    sprites,
                    size: *size,
                    frames_per_second: *rate,
                    frame: 0,
                }
            }
            ElementKind::Button(b) => {
                let mut states = [None; 4];
                for (s, r) in states.iter_mut().zip(&b.states) {
                    *s = child(self, r);
                }
                let content = child(self, &b.content);
                Kind::Button {
                    states,
                    content,
                    colors: b.colors,
                    state: 0,
                }
            }
            ElementKind::Text(t) => Kind::Text(self.text_data(t)),
            ElementKind::Label(l) => Kind::Label {
                text: self.text_data(&l.text),
                icon: self.res.sprite(l.sprite_sheet.as_deref(), &l.sprite),
                icon_size: l.icon_size,
                text_alignment: l.text_alignment,
                orientation: l.orientation,
                min_text_width: l.min_text_width,
                gap: l.gap,
            },
            _ => Kind::Control(self.instantiate_control(&e.kind)),
        };
        let b = &e.base;
        let mut node = Node::new(kind);
        node.name = name;
        node.margins = b.margins;
        node.min_size = b.min_size;
        node.alignment = b.alignment;
        node.flags = b.flags;
        node.color = b.color;
        node.children = children;
        node.tooltip = self.res.string(b.string_table.as_deref(), &b.tooltip);
        node.tooltip_dialog = b.tooltip_dialog.clone();
        node.string_table = b.string_table.clone();
        node.border = child(self, &b.border);

        self.add(node)
    }

    /// Depth-first search for a node by element name below `root`.
    pub fn find(&self, root: NodeId, name: &str) -> Option<NodeId> {
        if self.nodes[root].name == name {
            return Some(root);
        }
        self.child_ids(root)
            .into_iter()
            .find_map(|c| self.find(c, name))
    }

    fn child_ids(&self, id: NodeId) -> Vec<NodeId> {
        let n = &self.nodes[id];
        let mut out = n.children.clone();
        out.extend(n.border);
        if let Kind::Control(c) = &n.kind {
            out.extend(c.children());
        }
        if let Kind::Button {
            states, content, ..
        } = &n.kind
        {
            out.extend(states.iter().flatten());
            out.extend(content.iter());
        }
        out
    }

    /// Return every node rendered or owned directly below this node, including
    /// control and button content that is stored outside `Node::children`.
    pub fn children(&self, id: NodeId) -> Vec<NodeId> {
        self.child_ids(id)
    }

    /// Replace the text of a Text/Label node, or of a Button's content.
    pub fn set_text(&mut self, id: NodeId, text: &str) {
        let target = match &self.nodes[id].kind {
            Kind::Button {
                content: Some(c), ..
            } => *c,
            _ => id,
        };
        match &mut self.nodes[target].kind {
            Kind::Text(t) | Kind::Label { text: t, .. } => t.text = text.to_string(),
            Kind::Control(Control::Edit(e)) => {
                e.text.text = text.to_string();
                // Display-only number elements (`NumberElement`, no max length) never scroll.
                e.cursor = if e.max_length > 0 {
                    text.chars().count()
                } else {
                    0
                };
            }
            Kind::Control(Control::Document(d)) => d.text = text.to_string(),
            Kind::Control(Control::Combo { button, .. } | Control::Check { button, .. }) => {
                let b = *button;
                self.set_text(b, text);
            }
            _ => log::warn!("set_text on non-text node {}", self.nodes[target].name),
        }
    }

    pub fn set_visible(&mut self, id: NodeId, visible: bool) {
        let f = &mut self.nodes[id].flags;
        *f = if visible {
            *f & !flags::HIDDEN
        } else {
            *f | flags::HIDDEN
        };
    }

    // ------------------------------------------------------------ tool widgets

    pub fn group(&mut self) -> NodeId {
        self.add(Node::new(Kind::Group))
    }

    pub fn ribbon(&mut self, vertical: bool, spacing: i32) -> NodeId {
        self.add(Node::new(Kind::Ribbon {
            vertical,
            uniform: false,
            spacing,
            cells: Vec::new(),
        }))
    }

    pub fn text(&mut self, font: &str, text: &str) -> NodeId {
        let font = self.res.font(font);
        let data = TextData {
            font,
            text: text.to_string(),
            max_width: 0,
            justify: 0,
            wrap: false,
            shadow: false,
            ellipsis_width: 0,
            layout: TextLayout::default(),
        };
        self.add(Node::new(Kind::Text(data)))
    }

    pub fn viewport(&mut self) -> NodeId {
        self.add(Node::new(Kind::Viewport))
    }

    pub fn picture(&mut self) -> NodeId {
        self.add(Node::new(Kind::Picture {
            texture: 0,
            size: [0, 0],
        }))
    }

    /// A scrolling list using the game font and the `White` dialog sprite for highlights.
    pub fn list(&mut self, font: &str) -> NodeId {
        let font_id = self.res.font(font);
        let row_height = font_id.map_or(16, |f| self.res.fonts[f].font.size + 4);
        let highlight = self
            .res
            .sprite(Some(r"Dialog\SpriteSheet.rsc:resource"), "White");
        self.add(Node::new(Kind::List(ListData {
            items: Vec::new(),
            selected: None,
            hovered: None,
            scroll: 0,
            font: font_id,
            row_height,
            highlight,
            rows: Vec::new(),
            reveal: false,
        })))
    }

    /// Create a simple horizontal slider with a built-in solid track and knob.
    pub fn slider(&mut self, range: [f32; 2]) -> NodeId {
        self.add(Node::new(Kind::Control(Control::Slider(SliderData {
            sprites: vec![None; 5],
            range,
            value: range[0],
            step: (range[1] - range[0]).abs() / 100.0,
            inset: 8,
            track_y: 0,
            number: None,
        }))))
    }

    pub fn set_list_items(&mut self, id: NodeId, items: Vec<String>) {
        if let Kind::List(l) = &mut self.nodes[id].kind {
            l.items = items;
            l.selected = None;
            l.hovered = None;
            l.scroll = 0;
            l.rows.clear();
        }
    }

    pub fn list_select(&mut self, id: NodeId, index: Option<usize>) {
        if let Kind::List(l) = &mut self.nodes[id].kind {
            l.selected = index.filter(|&i| i < l.items.len());
            l.reveal = true;
        }
    }

    fn reveal_selection(&mut self, id: NodeId) {
        let h = self.nodes[id].rect[3] - self.nodes[id].rect[1];
        if let Kind::List(l) = &mut self.nodes[id].kind {
            if let (true, Some(i)) = (l.reveal && h > 0, l.selected) {
                let top = i as i32 * l.row_height;
                if top < l.scroll {
                    l.scroll = top;
                } else if top + l.row_height > l.scroll + h {
                    l.scroll = top + l.row_height - h;
                }
                l.reveal = false;
            }
        }
    }

    // ------------------------------------------------------------ layout

    fn padded_size(&mut self, id: NodeId) -> [i32; 2] {
        self.compute_size(id);
        let n = &self.nodes[id];
        let mut w = (n.size[0] + n.margins[0] + n.margins[1]).max(n.min_size[0]);
        let mut h = (n.size[1] + n.margins[2] + n.margins[3]).max(n.min_size[1]);
        if n.flags & flags::SCREEN_WIDTH != 0 {
            w = self.screen[0];
        }
        if n.flags & flags::SCREEN_HEIGHT != 0 {
            h = self.screen[1];
        }
        [w, h]
    }

    /// `ComputeSize` for each element class.
    fn compute_size(&mut self, id: NodeId) {
        let children = self.nodes[id].children.clone();
        let size = match self.nodes[id].kind.clone() {
            Kind::Group => {
                let mut s = [0, 0];
                for c in children {
                    if !self.nodes[c].visible() {
                        continue;
                    }
                    let p = self.padded_size(c);
                    s = [s[0].max(p[0]), s[1].max(p[1])];
                }
                s
            }
            Kind::Ribbon {
                vertical,
                uniform,
                spacing,
                ..
            } => {
                let (mut max, mut sum, mut cells, mut non_empty) = ([0, 0], [0, 0], Vec::new(), 0);
                for &c in &children {
                    if !self.nodes[c].visible() {
                        cells.push(0);
                        continue;
                    }
                    let p = self.padded_size(c);
                    max = [max[0].max(p[0]), max[1].max(p[1])];
                    sum = [sum[0] + p[0], sum[1] + p[1]];
                    cells.push(if vertical { p[1] } else { p[0] });
                    if p[0] > 0 && p[1] > 0 {
                        non_empty += 1;
                    }
                }
                if uniform {
                    let cell = if vertical { max[1] } else { max[0] };
                    cells.iter_mut().for_each(|c| *c = cell);
                    sum = [
                        children.len() as i32 * max[0],
                        children.len() as i32 * max[1],
                    ];
                }
                let gaps = spacing * (non_empty - 1).max(0);
                if let Kind::Ribbon { cells: stored, .. } = &mut self.nodes[id].kind {
                    *stored = cells;
                }
                if vertical {
                    [max[0], sum[1] + gaps]
                } else {
                    [sum[0] + gaps, max[1]]
                }
            }
            Kind::Border { sprites } => {
                let w = |i: usize| sprites[i].map_or(0, |s| s.width);
                let h = |i: usize| sprites[i].map_or(0, |s| s.height);
                [
                    (w(0) + w(2)).max(w(6) + w(8)),
                    (h(0) + h(6)).max(h(2) + h(8)),
                ]
            }
            Kind::Lines { size, .. } => size,
            Kind::Image { sprite, size } => [
                if size[0] > 0 {
                    size[0]
                } else {
                    sprite.map_or(0, |s| s.width)
                },
                if size[1] > 0 {
                    size[1]
                } else {
                    sprite.map_or(0, |s| s.height)
                },
            ],
            Kind::AnimatedImage {
                sprites,
                size,
                frame,
                ..
            } => {
                let sprite = sprites.get(frame).copied().flatten();
                [
                    if size[0] > 0 {
                        size[0]
                    } else {
                        sprite.map_or(0, |s| s.width)
                    },
                    if size[1] > 0 {
                        size[1]
                    } else {
                        sprite.map_or(0, |s| s.height)
                    },
                ]
            }
            Kind::Button {
                states,
                content,
                state,
                ..
            } => {
                let mut s = content.map_or([0, 0], |c| self.padded_size(c));
                if let Some(st) = states[state].or(states[0]) {
                    let p = self.padded_size(st);
                    s = [s[0].max(p[0]), s[1].max(p[1])];
                }
                let n = &self.nodes[id];
                [s[0].max(n.min_size[0]), s[1].max(n.min_size[1])]
            }
            Kind::Text(mut t) => {
                self.layout_text(&mut t);
                let s = [t.layout.width, t.layout.height];
                self.nodes[id].kind = Kind::Text(t);
                s
            }
            Kind::Label {
                mut text,
                icon,
                icon_size,
                orientation,
                min_text_width,
                gap,
                ..
            } => {
                self.layout_text(&mut text);
                let (tw, th) = (
                    text.layout.width.max(if text.text.is_empty() {
                        0
                    } else {
                        min_text_width
                    }),
                    text.layout.height,
                );
                let iw = if icon_size[0] > 0 {
                    icon_size[0]
                } else {
                    icon.map_or(0, |s| s.width)
                };
                let ih = if icon_size[1] > 0 {
                    icon_size[1]
                } else {
                    icon.map_or(0, |s| s.height)
                };
                if let Kind::Label { text: t, .. } = &mut self.nodes[id].kind {
                    *t = text;
                }
                if orientation > 1 {
                    [iw.max(tw), gap + ih + th]
                } else {
                    [gap + iw + tw, ih.max(th)]
                }
            }
            Kind::Picture { .. } | Kind::Viewport | Kind::List(_) => {
                for c in children {
                    self.compute_size(c);
                }
                self.nodes[id].min_size
            }
            Kind::Unsupported(_) => [0, 0],
            Kind::Control(c) => self.measure_control(id, c),
        };
        self.nodes[id].size = [
            size[0].max(self.nodes[id].min_size[0]),
            size[1].max(self.nodes[id].min_size[1]),
        ];
        if let Some(b) = self.nodes[id].border {
            self.compute_size(b);
        }
    }

    fn layout_text(&self, t: &mut TextData) {
        let mut display = t.text.clone();
        if !t.wrap && t.ellipsis_width > 0 {
            if let Some(f) = t.font {
                if TextLayout::compute(&self.res, f, &display, None, 0).width > t.ellipsis_width {
                    while !display.is_empty()
                        && TextLayout::compute(&self.res, f, &format!("{display}..."), None, 0)
                            .width
                            > t.ellipsis_width
                    {
                        display.pop();
                    }
                    display.push_str("...");
                }
            }
        }
        t.layout = match t.font {
            Some(f) => TextLayout::compute(
                &self.res,
                f,
                &display,
                t.wrap.then_some(t.max_width),
                t.justify,
            ),
            None => TextLayout::default(),
        };
    }

    /// Generic placement shared by most `ComputeLayout` implementations.
    fn place(&mut self, id: NodeId, mut l: i32, mut t: i32, r: i32, b: i32) {
        let n = &mut self.nodes[id];
        let (mut w, mut h) = (n.size[0], n.size[1]);
        if r < w + l {
            l = r - w;
        }
        if b < h + t {
            t = b - h;
        }
        let [ml, mr, mt, mb] = n.margins;
        let (mut x, mut y) = (l + ml, t + mt);
        let (aw, ah) = ((r - mr) - x, (b - mb) - y);
        if n.flags & flags::FILL_WIDTH != 0 {
            w = aw;
        }
        if n.flags & flags::FILL_HEIGHT != 0 {
            h = ah;
        }
        match n.alignment % 3 {
            1 => x += (aw >> 1) - (w >> 1),
            2 => x = (r - mr) - w,
            _ => {}
        }
        match n.alignment / 3 {
            1 => y += (ah >> 1) - (h >> 1),
            2 => y = (b - mb) - h,
            _ => {}
        }
        n.rect = [x, y, x + w, y + h];
    }

    /// Size and lay out `root` inside the given rectangle.
    pub fn layout(&mut self, root: NodeId, rect: [i32; 4]) {
        self.compute_size(root);
        self.compute_layout(root, rect[0], rect[1], rect[2], rect[3]);
    }

    /// Lay out a dialog's root element: placed at its own size by its alignment within `rect`
    /// (`DialogInstance::ComputeLayout`, 0x180116620), unlike ordinary groups which fill.
    pub fn layout_dialog(&mut self, root: NodeId, rect: [i32; 4]) {
        self.compute_size(root);
        if !matches!(self.nodes[root].kind, Kind::Group) {
            self.compute_layout(root, rect[0], rect[1], rect[2], rect[3]);
            return;
        }
        self.place(root, rect[0], rect[1], rect[2], rect[3]);
        let rc = self.nodes[root].rect;
        if let Some(border) = self.nodes[root].border {
            self.compute_layout(border, rc[0], rc[1], rc[2], rc[3]);
        }
        for c in self.nodes[root].children.clone() {
            if self.nodes[c].visible() {
                self.compute_layout(c, rc[0], rc[1], rc[2], rc[3]);
            }
        }
    }

    /// True if the current tree contains an animated image with frames and a positive frame rate.
    pub fn has_animations(&self) -> bool {
        self.nodes.iter().any(|node| {
            matches!(
                &node.kind,
                Kind::AnimatedImage {
                    sprites,
                    frames_per_second,
                    ..
                } if sprites.len() > 1 && *frames_per_second > 0
            )
        })
    }

    /// Select each animated image's frame at the supplied elapsed time in seconds.
    pub fn update_animations(&mut self, elapsed_seconds: f32) {
        for node in &mut self.nodes {
            if let Kind::AnimatedImage {
                sprites,
                frames_per_second,
                frame,
                ..
            } = &mut node.kind
            {
                if !sprites.is_empty() && *frames_per_second > 0 {
                    *frame = ((elapsed_seconds.max(0.0) * *frames_per_second as f32).floor()
                        as usize)
                        % sprites.len();
                }
            }
        }
    }

    /// Current frame, frame count, and frame rate for the first animated image in the tree.
    pub fn animation_info(&self) -> Option<(usize, usize, i32)> {
        self.nodes.iter().find_map(|node| match &node.kind {
            Kind::AnimatedImage {
                sprites,
                frames_per_second,
                frame,
                ..
            } if !sprites.is_empty() => Some((*frame, sprites.len(), *frames_per_second)),
            _ => None,
        })
    }

    /// Select the same frame in every animated image, wrapping each image's own frame count.
    pub fn set_animation_frame(&mut self, frame: usize) {
        for node in &mut self.nodes {
            if let Kind::AnimatedImage {
                sprites,
                frame: current,
                ..
            } = &mut node.kind
            {
                if !sprites.is_empty() {
                    *current = frame % sprites.len();
                }
            }
        }
    }

    /// `ComputeLayout` for each element class.
    fn compute_layout(&mut self, id: NodeId, l: i32, t: i32, r: i32, b: i32) {
        let slot = [l, t, r, b];
        if matches!(self.nodes[id].kind, Kind::Control(_)) {
            self.place(id, l, t, r, b);
            self.layout_control(id);
        } else {
            match self.nodes[id].kind.clone() {
                Kind::Group
                | Kind::Viewport
                | Kind::List(_)
                | Kind::Picture { .. }
                | Kind::Image { .. }
                | Kind::Lines { .. }
                | Kind::AnimatedImage { .. } => {
                    // GroupElement fills the space offered, less its margins.
                    let n = &mut self.nodes[id];
                    let [ml, mr, mt, mb] = n.margins;
                    if matches!(n.kind, Kind::Group)
                        || n.flags & (flags::FILL_WIDTH | flags::FILL_HEIGHT) != 0
                    {
                        n.rect = [l + ml, t + mt, r - mr, b - mb];
                        if !matches!(n.kind, Kind::Group) {
                            let full = n.rect;
                            self.place(id, l, t, r, b);
                            let n = &mut self.nodes[id];
                            if n.flags & flags::FILL_WIDTH != 0 {
                                (n.rect[0], n.rect[2]) = (full[0], full[2]);
                            }
                            if n.flags & flags::FILL_HEIGHT != 0 {
                                (n.rect[1], n.rect[3]) = (full[1], full[3]);
                            }
                        }
                    } else {
                        self.place(id, l, t, r, b);
                    }
                    self.reveal_selection(id);
                    let rc = self.nodes[id].rect;
                    for c in self.nodes[id].children.clone() {
                        if self.nodes[c].visible() {
                            self.compute_layout(c, rc[0], rc[1], rc[2], rc[3]);
                        }
                    }
                }
                Kind::Border { .. } => {
                    let n = &mut self.nodes[id];
                    let [ml, mr, mt, mb] = n.margins;
                    n.rect = [l + ml, t + mt, r - mr, b - mb];
                }
                Kind::Ribbon {
                    vertical,
                    spacing,
                    cells,
                    ..
                } => {
                    self.place(id, l, t, r, b);
                    let rc = self.nodes[id].rect;
                    let fill_last = self.nodes[id].flags
                        & if vertical {
                            flags::FILL_HEIGHT
                        } else {
                            flags::FILL_WIDTH
                        }
                        != 0;
                    let children = self.nodes[id].children.clone();
                    let mut offset = 0;
                    for (i, c) in children.iter().copied().enumerate() {
                        if !self.nodes[c].visible() {
                            continue;
                        }
                        let cell = cells.get(i).copied().unwrap_or(0);
                        let last = i + 1 == children.len();
                        let p = self.padded_size(c);
                        if vertical {
                            let bottom = if fill_last && last {
                                rc[3]
                            } else {
                                rc[1] + offset + cell
                            };
                            self.compute_layout(c, rc[0], rc[1] + offset, rc[2], bottom);
                        } else {
                            let right = if fill_last && last {
                                rc[2]
                            } else {
                                rc[0] + offset + cell
                            };
                            self.compute_layout(c, rc[0] + offset, rc[1], right, rc[3]);
                        }
                        offset += cell;
                        if p[0] > 0 && p[1] > 0 {
                            offset += spacing;
                        }
                    }
                }
                Kind::Button {
                    states, content, ..
                } => {
                    self.place(id, l, t, r, b);
                    let rc = self.nodes[id].rect;
                    for c in states.iter().flatten().chain(content.iter()) {
                        self.compute_layout(*c, rc[0], rc[1], rc[2], rc[3]);
                    }
                }
                Kind::Text(_) | Kind::Label { .. } | Kind::Unsupported(_) => {
                    self.place(id, l, t, r, b)
                }
                Kind::Control(_) => unreachable!(),
            }
        }
        // `Element::ComputeLayout` (0x180119160) and its subclasses hand the border the slot
        // the element was given, before the element's own margins: a padded text sits inside
        // its box rather than the box inside the text.
        if let Some(border) = self.nodes[id].border {
            self.compute_layout(border, slot[0], slot[1], slot[2], slot[3]);
        }
    }

    // ------------------------------------------------------------ drawing

    pub fn draw(&self, root: NodeId) -> Vec<Quad> {
        let mut out = Vec::new();
        self.draw_node(root, 0xFFFF_FFFF, None, &mut out);
        self.draw_popups(root, &mut out);
        self.draw_tooltip(root, &mut out);
        out
    }

    fn sprite_quad(
        out: &mut Vec<Quad>,
        s: &SpriteRef,
        rect: [i32; 4],
        color: u32,
        clip: Option<[i32; 4]>,
    ) {
        if rect[2] <= rect[0] || rect[3] <= rect[1] {
            return;
        }
        out.push(Quad {
            texture: Texture::Sheet(s.sheet),
            rect: rect.map(|v| v as f32),
            uv: s.uv,
            color: argb_to_rgba(color, 255),
            clip,
        });
    }

    fn draw_text(
        &self,
        t: &TextData,
        origin: [i32; 2],
        color: u32,
        clip: Option<[i32; 4]>,
        out: &mut Vec<Quad>,
    ) {
        let Some(font) = t.font else { return };
        let Some(sheet) = self.res.fonts[font].sheet else {
            return;
        };
        let sprites = &self.res.sheets[sheet].sheet.sprites;
        if t.shadow {
            let mut shadow = t.clone();
            shadow.shadow = false;
            self.draw_text(
                &shadow,
                [origin[0] + 1, origin[1] + 1],
                color & 0xFF000000,
                clip,
                out,
            );
        }
        for g in &t.layout.glyphs {
            let Some(sp) = sprites.get(g.glyph) else {
                continue;
            };
            let (x, y) = (origin[0] + g.x, origin[1] + g.y);
            out.push(Quad {
                texture: Texture::Sheet(sheet),
                rect: [
                    x as f32,
                    y as f32,
                    (x + sp.width) as f32,
                    (y + sp.height) as f32,
                ],
                uv: sp.uv,
                color: argb_to_rgba(color, 255),
                clip,
            });
        }
    }

    fn draw_node(&self, id: NodeId, inherited: u32, clip: Option<[i32; 4]>, out: &mut Vec<Quad>) {
        let n = &self.nodes[id];
        if !n.visible() {
            return;
        }
        let mut color = modulate(n.color, inherited);
        if n.flags & flags::DIM != 0 {
            color = (color & 0x00FF_FFFF) | ((color >> 25) << 24);
        }
        let [l, t, r, b] = n.rect;
        if let Some(border) = n.border {
            self.draw_node(border, color, clip, out);
        }
        match &n.kind {
            Kind::Control(c) => self.draw_control(id, c, color, clip, out),
            Kind::Border { sprites } => {
                let w = |i: usize| sprites[i].map_or(0, |s| s.width);
                let h = |i: usize| sprites[i].map_or(0, |s| s.height);
                let cells = [
                    [l, t, l + w(0), t + h(0)],
                    [l + w(0), t, r - w(2), t + h(1)],
                    [r - w(2), t, r, t + h(2)],
                    [l, t + h(0), l + w(3), b - h(6)],
                    [l + w(3), t + h(1), r - w(5), b - h(7)],
                    [r - w(5), t + h(2), r, b - h(8)],
                    [l, b - h(6), l + w(6), b],
                    [l + w(6), b - h(7), r - w(8), b],
                    [r - w(8), b - h(8), r, b],
                ];
                for (s, rect) in sprites.iter().zip(cells) {
                    if let Some(s) = s {
                        Self::sprite_quad(out, s, rect, color, clip);
                    }
                }
            }
            Kind::Image {
                sprite: Some(s), ..
            } => Self::sprite_quad(out, s, n.rect, color, clip),
            Kind::Lines {
                sprite: Some(s),
                lines,
                ..
            } => {
                // One-pixel steps along each segment, clipped to the element.
                let r = n.rect;
                let clip = Some(match clip {
                    Some(c) => [
                        c[0].max(r[0]),
                        c[1].max(r[1]),
                        c[2].min(r[2]),
                        c[3].min(r[3]),
                    ],
                    None => r,
                });
                for &[x0, y0, x1, y1] in lines {
                    let steps = (x1 - x0).abs().max((y1 - y0).abs()).max(1);
                    for i in 0..=steps {
                        let x = r[0] + x0 + (x1 - x0) * i / steps;
                        let y = r[1] + y0 + (y1 - y0) * i / steps;
                        Self::sprite_quad(out, s, [x, y, x + 1, y + 1], color, clip);
                    }
                }
            }
            Kind::AnimatedImage { sprites, frame, .. } => {
                if let Some(Some(sprite)) = sprites.get(*frame) {
                    Self::sprite_quad(out, sprite, n.rect, color, clip);
                }
            }
            Kind::Button {
                states,
                content,
                colors,
                state,
            } => {
                if let Some(s) = states[*state].or(states[0]) {
                    self.draw_node(s, color, clip, out);
                }
                if let Some(c) = content {
                    let tint = if colors[*state] != 0 {
                        modulate(colors[*state], color)
                    } else {
                        color
                    };
                    self.draw_node(*c, tint, clip, out);
                }
            }
            Kind::Text(text) => {
                let x = match text.justify {
                    1 => l + ((r - l) - text.layout.width) / 2,
                    2 => r - text.layout.width,
                    _ => l,
                };
                self.draw_text(text, [x, t], color, clip, out);
            }
            Kind::Label {
                text,
                icon,
                icon_size,
                text_alignment,
                orientation,
                gap,
                ..
            } => {
                let iw = if icon_size[0] > 0 {
                    icon_size[0]
                } else {
                    icon.map_or(0, |s| s.width)
                };
                let ih = if icon_size[1] > 0 {
                    icon_size[1]
                } else {
                    icon.map_or(0, |s| s.height)
                };
                let (tw, th) = (text.layout.width, text.layout.height);
                // Areas for the text and the icon by orientation (LabelElement::Draw).
                let (text_area, icon_pos) = match orientation {
                    0 => ([l, t, r - gap - iw, b], [r - iw, t + ((b - t) - ih) / 2]),
                    1 => ([l + iw + gap, t, r, b], [l, t + ((b - t) - ih) / 2]),
                    3 => ([l, t + ih + gap, r, b], [l + ((r - l) - iw) / 2, t]),
                    _ => ([l, t, r, b - ih - gap], [l + ((r - l) - iw) / 2, b - ih]),
                };
                let [al, at, ar, ab] = text_area;
                let x = match text_alignment % 3 {
                    1 => al + ((ar - al) - tw) / 2,
                    2 => ar - tw,
                    _ => al,
                };
                let y = match text_alignment / 3 {
                    1 => at + ((ab - at) - th) / 2,
                    2 => ab - th,
                    _ => at,
                };
                if let Some(s) = icon {
                    Self::sprite_quad(
                        out,
                        s,
                        [icon_pos[0], icon_pos[1], icon_pos[0] + iw, icon_pos[1] + ih],
                        color,
                        clip,
                    );
                }
                self.draw_text(text, [x, y], color, clip, out);
            }
            Kind::List(list) => {
                let list_clip = controls::intersect(clip, [l, t, r, b]);
                let first = (list.scroll / list.row_height.max(1)).max(0) as usize;
                let rows = ((b - t) / list.row_height.max(1) + 2) as usize;
                for i in first..(first + rows).min(list.items.len()) {
                    let y = t + i as i32 * list.row_height - list.scroll;
                    let row = [l, y, r, y + list.row_height];
                    let tint = if list.selected == Some(i) {
                        Some(0xC0C9_A24A)
                    } else if list.hovered == Some(i) {
                        Some(0x40FF_FFFF)
                    } else {
                        None
                    };
                    if let (Some(c), Some(s)) = (tint, &list.highlight) {
                        Self::sprite_quad(out, s, row, c, list_clip);
                    }
                    let layout = match list.font {
                        Some(f) => TextLayout::compute(&self.res, f, &list.items[i], None, 0),
                        None => continue,
                    };
                    let text = TextData {
                        font: list.font,
                        text: String::new(),
                        max_width: 0,
                        justify: 0,
                        wrap: false,
                        shadow: false,
                        ellipsis_width: 0,
                        layout,
                    };
                    let ty = y + (list.row_height - text.layout.height) / 2;
                    self.draw_text(&text, [l + 6, ty], color, list_clip, out);
                }
                // Scroll indicator.
                let total = list.items.len() as i32 * list.row_height;
                if total > b - t {
                    if let Some(s) = &list.highlight {
                        let h = ((b - t) * (b - t) / total).max(12);
                        let y = t + (list.scroll * ((b - t) - h)) / (total - (b - t)).max(1);
                        Self::sprite_quad(out, s, [r - 4, y, r, y + h], 0x80FF_FFFF, list_clip);
                    }
                }
            }
            Kind::Picture { texture, size } if size[0] > 0 && size[1] > 0 => {
                // Fit inside the rect, preserving aspect, never upscaling past 4x.
                let (aw, ah) = ((r - l) as f32, (b - t) as f32);
                let scale = (aw / size[0] as f32).min(ah / size[1] as f32).min(4.0);
                let (w, h) = (size[0] as f32 * scale, size[1] as f32 * scale);
                let (x, y) = (l as f32 + (aw - w) / 2.0, t as f32 + (ah - h) / 2.0);
                out.push(Quad {
                    texture: Texture::External(*texture),
                    rect: [x, y, x + w, y + h],
                    uv: [0.0, 0.0, 1.0, 1.0],
                    color: [255, 255, 255, 255],
                    clip,
                });
            }
            _ => {}
        }
        for &c in &n.children {
            self.draw_node(c, color, clip, out);
        }
    }
}
