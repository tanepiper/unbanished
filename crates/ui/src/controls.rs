//! The controls with state: sliders, scroll bars, progress bars, edit boxes, graphs, tables, tabs,
//! containers, check boxes, combo boxes, spinners, pages, radio groups and documents.
//!
//! The order of the sprites and the layout follow the engine's own element classes. The
//! application supplies list rows, graph samples and combo choices through the setters on
//! [`Ui`](crate::Ui).
mod measure;
mod paint;
mod setup;
mod values;
pub(crate) use paint::scroll_thumb;

use crate::*;
use banished_formats::ui::{self, ElementKind};

#[derive(Debug, Clone)]
/// The state of a slider.
pub struct SliderData {
    /// The sprites: the track's ends and middle, the knob, and the knob under the pointer.
    pub sprites: Vec<Option<SpriteRef>>,
    /// The smallest and the largest value.
    pub range: [f32; 2],
    /// The value.
    pub value: f32,
    // +0x120; +0xb0
    /// Steps across the range (SliderDescription): a click on the track moves the value one of
    /// them.
    pub steps: i32,
    /// The space between the track's ends and the knob's travel, in pixels.
    pub inset: i32,
    /// The track's vertical position in the element, in pixels.
    pub track_y: i32,
    /// The number shown next to the slider, if it has one.
    pub number: Option<NodeId>,
    // +0x124
    /// The pointer is over the knob (it draws the fifth sprite).
    pub over_knob: bool,
    // +0x12c
    /// A drag from the knob (flag `0x200000`): the value and pointer x when it began.
    pub drag: Option<(f32, i32)>,
}

impl SliderData {
    /// One step of the range (a track click), a hundredth when the slider has none.
    pub fn step(&self) -> f32 {
        let range = self.range[1] - self.range[0];
        if 0 < self.steps {
            range / self.steps as f32
        } else {
            range.abs() / 100.0
        }
    }

    /// The knob's width and the length it travels along `rect` (`SliderElement::Update` and
    /// `Draw`: the width less twice the inset and twice half the knob's width).
    fn travel(&self, rect: [i32; 4]) -> (i32, i32) {
        let width = self.sprites.get(3).and_then(|s| *s).map_or(12, |s| s.width);
        (width, rect[2] - (width / 2 + self.inset) * 2 - rect[0])
    }

    /// The knob's rectangle in `rect`: its sprite's size, from the inset at the minimum.
    pub fn knob(&self, rect: [i32; 4]) -> [i32; 4] {
        let (width, travel) = self.travel(rect);
        let height = self
            .sprites
            .get(3)
            .and_then(|s| *s)
            .map_or(rect[3] - rect[1], |s| s.height);
        let range = self.range[1] - self.range[0];
        let offset = if range != 0.0 {
            (travel as f32 * ((self.value - self.range[0]) / range)) as i32
        } else {
            0
        };
        let x = rect[0] + self.inset + offset;
        [x, rect[1], x + width, rect[1] + height]
    }

    /// The value a drag from the knob gives with the pointer at `x`.
    pub fn dragged(&self, rect: [i32; 4], x: i32) -> Option<f32> {
        let (start, from) = self.drag?;
        let (_, travel) = self.travel(rect);
        Some(start - ((from - x) as f32 / travel as f32) * (self.range[1] - self.range[0]))
    }
}

#[derive(Debug, Clone)]
/// The state of an edit box.
pub struct EditData {
    /// The text, and its layout.
    pub text: TextData,
    /// Cursor in Unicode scalar values, never byte offsets.
    pub cursor: usize,
    /// The most characters it takes.
    pub max_length: usize,
    /// Whether it takes a number.
    pub numeric: bool,
    /// The smallest and the largest number, for a number box.
    pub range: Option<[f32; 2]>,
    // +0xd4; +0x7c
    /// Drawn as `*`s (EditDescription).
    pub password: bool,
    // +0xd8
    /// The characters it takes, any when empty (a NumberEdit's digits, and `-` when it may go below
    /// zero).
    pub allowed: String,
    // +0xe8; +0xf0
    /// The caret's blink (flipping every half second).
    pub caret: bool,
    /// The time since the caret last changed, in seconds.
    pub blink: f32,
}

#[derive(Debug, Clone)]
/// A child of a layout element, with its anchors.
pub struct LayoutChild {
    /// The child.
    pub node: NodeId,
    /// How it is placed: its anchors and offsets.
    pub spec: ui::LayoutItem,
}

#[derive(Debug, Clone)]
/// A control and its state.
pub enum Control {
    /// A slider.
    Slider(SliderData),
    /// A scroll bar.
    Scroll {
        /// The sprites of the track and the thumb.
        sprites: Vec<Option<SpriteRef>>,
        /// A vertical bar when true.
        vertical: bool,
        /// The position, from 0 to 1.
        value: f32,
        /// The share of the content that is visible, from 0 to 1.
        page: f32,
        // +0x15c; +0xbc
        /// The bar's inset at both ends of the track (ScrollDescription).
        inset: i32,
        // +0x154
        /// The thumb's colour.
        thumb_color: u32,
        // +0x16c
        /// A drag from the thumb (flag `0x200000`): the value and the pointer's coordinate along
        /// the bar when it began.
        drag: Option<(f32, i32)>,
    },
    /// A progress bar.
    Progress {
        /// The sprites of the fill and the background.
        sprites: Vec<Option<SpriteRef>>,
        /// The background's colour, `0xAARRGGBB`.
        background_color: u32,
        /// The fill, from 0 to 1.
        value: f32,
        /// The element drawn on top (usually a label).
        content: Option<NodeId>,
    },
    /// An edit box.
    Edit(EditData),
    /// `GraphElement`: a frame, `y_divisions + 1` lines across (with ticks of `tick` px to
    /// the left) and `x_divisions + 1` down (ticks below), then each graph's points from the
    /// left edge to the right, `y_range` at the top.
    Graph {
        /// The size in pixels.
        size: [i32; 2],
        /// The sprite the lines are drawn with.
        sprite: Option<SpriteRef>,
        /// The length of the ticks, in pixels.
        tick: i32,
        /// The value at the top of the graph.
        y_range: f32,
        /// The number of divisions across.
        x_divisions: i32,
        /// The number of divisions down.
        y_divisions: i32,
        /// `AddGraph(color)` / `AddGraphPoint`.
        series: Vec<(u32, Vec<i32>)>,
    },
    /// A table of cells in rows and columns.
    Table {
        /// Each cell: its row, its column and its element.
        cells: Vec<(usize, usize, NodeId)>,
        /// The width of each column after the last layout.
        columns: Vec<i32>,
        /// The height of each row after the last layout.
        rows: Vec<i32>,
        /// The space between the cells, in pixels.
        spacing: i32,
    },
    /// Children placed by anchors relative to the element.
    Layout(Vec<LayoutChild>),
    /// Tabs: a header for each tab, and the page of the selected one.
    Tabs {
        /// The elements drawn around the tabs.
        decoration: [Option<NodeId>; 3],
        /// Each tab: its header and its page.
        tabs: Vec<(NodeId, NodeId)>,
        /// The selected tab.
        selected: usize,
        /// The space between the headers, in pixels.
        gap: i32,
        /// The headers' height, in pixels.
        header_height: i32,
    },
    /// A window onto its content, cut at its edges.
    Clip {
        /// The content.
        content: Option<NodeId>,
        /// The window's size, in pixels.
        size: [i32; 2],
        /// The content's offset, in pixels.
        offset: [i32; 2],
    },
    /// A scrolling area: its content and its scroll bars.
    Container {
        /// The content.
        content: Option<NodeId>,
        /// The horizontal and the vertical scroll bar.
        bars: [Option<NodeId>; 2],
        /// The size, in pixels.
        size: [i32; 2],
        /// The space between the content and the bars, in pixels.
        spacing: i32,
        /// How far the content is scrolled, in pixels.
        offset: [i32; 2],
        /// The visible part of the content: x0, y0, x1, y1.
        view: [i32; 4],
        // +0xd0; ScrollElement +0x160
        /// `ContainerElement::Update`: the wheel's frames still to scroll (±7 a notch) and the
        /// vertical bar's value (`ScrollElement`, in pixels).
        wheel: i32,
        /// The vertical bar's position, in pixels.
        fine: f32,
    },
    /// A check box.
    Check {
        /// The box.
        button: NodeId,
        /// The tick, shown when checked.
        mark: Option<NodeId>,
        /// Whether it is checked.
        checked: bool,
    },
    /// A combo box: a button that opens a list.
    Combo {
        /// The button that shows the choice.
        button: NodeId,
        /// The list of choices.
        list: NodeId,
        /// Whether the list is open.
        open: bool,
    },
    /// A number with up and down arrows.
    Spinner {
        /// The number and the two arrows.
        parts: [Option<NodeId>; 3],
        /// The number.
        value: i32,
        /// How much an arrow changes it.
        step: i32,
    },
    /// Pages, of which one is shown.
    Page {
        /// The pages.
        children: Vec<NodeId>,
        /// The page shown.
        selected: usize,
    },
    /// Radio buttons, of which one is selected.
    Radio {
        /// The buttons.
        children: Vec<NodeId>,
        /// The selected button.
        selected: usize,
    },
    /// A document with markup: the help pages and the tutorials.
    Document(document::Document),
}

impl Control {
    pub(crate) fn children(&self) -> Vec<NodeId> {
        match self {
            Self::Slider(s) => s.number.into_iter().collect(),
            Self::Progress { content, .. } | Self::Clip { content, .. } => {
                content.iter().copied().collect()
            }
            Self::Container { content, bars, .. } => content
                .iter()
                .chain(bars.iter().flatten())
                .copied()
                .collect(),
            Self::Table { cells, .. } => cells.iter().map(|c| c.2).collect(),
            Self::Layout(items) => items.iter().map(|i| i.node).collect(),
            Self::Tabs {
                decoration, tabs, ..
            } => decoration
                .iter()
                .flatten()
                .copied()
                .chain(tabs.iter().flat_map(|&(a, b)| [a, b]))
                .collect(),
            Self::Check { button, mark, .. } => std::iter::once(*button).chain(*mark).collect(),
            Self::Combo { button, list, .. } => vec![*button, *list],
            Self::Spinner { parts, .. } => parts.iter().flatten().copied().collect(),
            Self::Page { children, .. } | Self::Radio { children, .. } => children.clone(),
            _ => Vec::new(),
        }
    }
    pub(crate) fn active_children(&self) -> Vec<NodeId> {
        match self {
            Self::Page { children, selected } => {
                children.get(*selected).copied().into_iter().collect()
            }
            Self::Tabs {
                decoration,
                tabs,
                selected,
                ..
            } => decoration
                .iter()
                .flatten()
                .copied()
                .chain(tabs.iter().map(|t| t.0))
                .chain(tabs.get(*selected).map(|t| t.1))
                .collect(),
            Self::Combo { button, list, open } => std::iter::once(*button)
                .chain(open.then_some(*list))
                .collect(),
            Self::Check {
                button,
                mark,
                checked,
            } => std::iter::once(*button)
                .chain(if *checked { *mark } else { None })
                .collect(),
            _ => self.children(),
        }
    }
}

impl Ui {}

pub(crate) fn intersect(a: Option<[i32; 4]>, b: [i32; 4]) -> Option<[i32; 4]> {
    Some(if let Some(a) = a {
        [
            a[0].max(b[0]),
            a[1].max(b[1]),
            a[2].min(b[2]),
            a[3].min(b[3]),
        ]
    } else {
        b
    })
}
pub(crate) fn format_number(v: f32) -> String {
    if v.fract().abs() < 0.00001 {
        format!("{v:.0}")
    } else {
        format!("{v:.2}")
    }
}
pub(crate) fn fraction(v: f32, range: [f32; 2]) -> f32 {
    if range[0] == range[1] {
        0.0
    } else {
        ((v - range[0]) / (range[1] - range[0])).clamp(0.0, 1.0)
    }
}
