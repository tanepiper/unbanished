//! Stateful controls. Sprite ordering and layout follow the Ghidra-exported UI methods;
//! application-owned lists, graph samples and combo choices are supplied through setters.
use crate::*;
use banished_formats::ui::{self, ElementKind};

#[derive(Debug, Clone)]
pub struct SliderData {
    pub sprites: Vec<Option<SpriteRef>>,
    pub range: [f32; 2],
    pub value: f32,
    pub step: f32,
    pub inset: i32,
    pub track_y: i32,
    pub number: Option<NodeId>,
}

#[derive(Debug, Clone)]
pub struct EditData {
    pub text: TextData,
    /// Cursor in Unicode scalar values, never byte offsets.
    pub cursor: usize,
    pub max_length: usize,
    pub numeric: bool,
    pub range: Option<[f32; 2]>,
    pub scroll: i32,
}

#[derive(Debug, Clone)]
pub struct LayoutChild {
    pub node: NodeId,
    pub spec: ui::LayoutItem,
}

#[derive(Debug, Clone)]
pub enum Control {
    Slider(SliderData),
    Scroll {
        sprites: Vec<Option<SpriteRef>>,
        vertical: bool,
        value: f32,
        page: f32,
    },
    Progress {
        sprites: Vec<Option<SpriteRef>>,
        background_color: u32,
        value: f32,
        content: Option<NodeId>,
    },
    Edit(EditData),
    Graph {
        size: [i32; 2],
        sprite: Option<SpriteRef>,
        color: u32,
        series: Vec<Vec<f32>>,
    },
    Table {
        cells: Vec<(usize, usize, NodeId)>,
        columns: Vec<i32>,
        rows: Vec<i32>,
        spacing: i32,
    },
    Layout(Vec<LayoutChild>),
    Tabs {
        decoration: [Option<NodeId>; 3],
        tabs: Vec<(NodeId, NodeId)>,
        selected: usize,
        gap: i32,
        header_height: i32,
    },
    Clip {
        content: Option<NodeId>,
        size: [i32; 2],
        offset: [i32; 2],
    },
    Container {
        content: Option<NodeId>,
        bars: [Option<NodeId>; 2],
        size: [i32; 2],
        spacing: i32,
        offset: [i32; 2],
        view: [i32; 4],
    },
    Check {
        button: NodeId,
        mark: Option<NodeId>,
        checked: bool,
    },
    Combo {
        button: NodeId,
        list: NodeId,
        open: bool,
    },
    Spinner {
        parts: [Option<NodeId>; 3],
        value: i32,
        step: i32,
    },
    Page {
        children: Vec<NodeId>,
        selected: usize,
    },
    Radio {
        children: Vec<NodeId>,
        selected: usize,
    },
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

impl Ui {
    pub(crate) fn make_text(&mut self, font: Option<&str>, text: String) -> TextData {
        TextData {
            font: font.and_then(|f| self.res.font(f)),
            text,
            max_width: 0,
            justify: 0,
            wrap: false,
            shadow: false,
            ellipsis_width: 0,
            layout: TextLayout::default(),
        }
    }
    fn control_button(&mut self, b: &ui::Button) -> NodeId {
        let states =
            std::array::from_fn(|i| b.states[i].as_deref().and_then(|r| self.instantiate(r)));
        let content = b.content.as_deref().and_then(|r| self.instantiate(r));
        self.add(Node::new(Kind::Button {
            states,
            content,
            colors: b.colors,
            state: 0,
        }))
    }
    pub(crate) fn instantiate_control(&mut self, kind: &ElementKind) -> Control {
        let child = |ui: &mut Ui, r: &Option<String>| r.as_deref().and_then(|r| ui.instantiate(r));
        let sprites = |ui: &mut Ui, sheet: &Option<String>, names: &[String]| {
            names
                .iter()
                .map(|s| ui.res.sprite(sheet.as_deref(), s))
                .collect::<Vec<_>>()
        };
        match kind {
            ElementKind::Slider {
                track,
                sprite_sheet,
                sprites: names,
                unknown,
                range,
                unknown_b0,
            } => Control::Slider(SliderData {
                sprites: sprites(self, sprite_sheet, names),
                range: *range,
                value: range[0],
                step: if *unknown_b0 > 0 {
                    (range[1] - range[0]) / *unknown_b0 as f32
                } else {
                    0.0
                },
                inset: unknown[1].max(0),
                track_y: unknown[0],
                number: child(self, track),
            }),
            ElementKind::Scroll {
                sprite_sheet,
                sprites: names,
                vertical,
                ..
            } => Control::Scroll {
                sprites: sprites(self, sprite_sheet, names),
                vertical: *vertical,
                value: 0.0,
                page: 0.25,
            },
            ElementKind::Progress {
                color,
                sprite_sheet,
                sprites: names,
                unknown_ref,
            } => Control::Progress {
                sprites: sprites(self, sprite_sheet, names),
                background_color: *color,
                value: 0.0,
                content: child(self, unknown_ref),
            },
            ElementKind::Edit {
                font,
                text,
                max_length,
                ..
            } => Control::Edit(EditData {
                text: self.make_text(font.as_deref(), text.clone()),
                cursor: text.chars().count(),
                max_length: if *max_length > 0 {
                    *max_length as usize
                } else {
                    usize::MAX
                },
                numeric: false,
                range: None,
                scroll: 0,
            }),
            ElementKind::NumberEdit { font, unknown, .. } => Control::Edit(EditData {
                text: self.make_text(font.as_deref(), unknown[0].to_string()),
                cursor: 1,
                max_length: unknown[3].max(1) as usize,
                numeric: true,
                range: Some([unknown[1] as f32, unknown[2] as f32]),
                scroll: 0,
            }),
            ElementKind::Number {
                font,
                unknown_float,
                ..
            } => Control::Edit(EditData {
                text: self.make_text(font.as_deref(), format_number(*unknown_float)),
                cursor: 0,
                max_length: 0,
                numeric: true,
                range: None,
                scroll: 0,
            }),
            ElementKind::Graph {
                unknown,
                unknown_u32,
                sprite_sheet,
                sprite,
            } => Control::Graph {
                size: [unknown[0], unknown[1]],
                sprite: self.res.sprite(sprite_sheet.as_deref(), sprite),
                color: *unknown_u32,
                series: Vec::new(),
            },
            ElementKind::Table { unknown, cells } => {
                let cols = unknown[0].clamp(0, 256) as usize;
                let rows = unknown[1].clamp(0, 4096) as usize;
                let mut out = Vec::new();
                for y in 0..rows {
                    for x in 0..cols {
                        // Specific cell > column/row template > table default.
                        let template = cells
                            .iter()
                            .filter(|cell| {
                                (cell.column < 0 || cell.column as usize == x)
                                    && (cell.row < 0 || cell.row as usize == y)
                            })
                            .max_by_key(|cell| (cell.column >= 0) as u8 + (cell.row >= 0) as u8);
                        if let Some(c) = template.and_then(|cell| child(self, &cell.element)) {
                            out.push((x, y, c));
                        }
                    }
                }
                Control::Table {
                    cells: out,
                    columns: vec![0; cols],
                    rows: vec![0; rows],
                    spacing: unknown[2],
                }
            }
            ElementKind::Layout { items } => Control::Layout(
                items
                    .iter()
                    .filter_map(|spec| {
                        child(self, &spec.element).map(|node| LayoutChild {
                            node,
                            spec: spec.clone(),
                        })
                    })
                    .collect(),
            ),
            ElementKind::Tab {
                refs,
                tabs,
                unknown,
                ..
            } => Control::Tabs {
                decoration: std::array::from_fn(|i| child(self, &refs[i])),
                tabs: tabs
                    .iter()
                    .filter_map(|(a, b)| Some((child(self, a)?, child(self, b)?)))
                    .collect(),
                selected: 0,
                gap: *unknown,
                header_height: 0,
            },
            ElementKind::ClipRegion { unknown, content } => Control::Clip {
                content: child(self, content),
                size: *unknown,
                offset: [0, 0],
            },
            ElementKind::Container { unknown, refs, .. } => Control::Container {
                content: child(self, &refs[2]),
                bars: [child(self, &refs[0]), child(self, &refs[1])],
                size: [unknown[0], unknown[1]],
                spacing: unknown[2],
                offset: [0, 0],
                view: [0; 4],
            },
            ElementKind::Check { button, check } => Control::Check {
                button: self.control_button(button),
                mark: child(self, check),
                checked: false,
            },
            ElementKind::Combo { button, refs } => {
                let b = self.control_button(button);
                self.set_text(b, "");
                let font = refs[1]
                    .as_deref()
                    .and_then(|r| self.res.element(r))
                    .and_then(|e| match &e.kind {
                        ElementKind::Text(t) => t.font.clone(),
                        _ => None,
                    })
                    .unwrap_or_else(|| r"Font\FontSmall.rsc:resource".into());
                let list = self.list(&font);
                // The third reference is an optional string table whose entries are the items
                // (e.g. `Dialog\StringTable.rsc:terrainSize`); code fills the other combos.
                if let Some(items) = refs[2].as_deref().and_then(|r| {
                    let data = self.res.archive.read_ref(r).ok()?;
                    banished_formats::ui::string_table(&data).ok()
                }) {
                    self.set_list_items(list, items.into_iter().map(|(_, v)| v).collect());
                }
                Control::Combo {
                    button: b,
                    list,
                    open: false,
                }
            }
            ElementKind::Spinner { refs, unknown } => Control::Spinner {
                parts: std::array::from_fn(|i| child(self, &refs[i])),
                value: 0,
                step: *unknown,
            },
            ElementKind::Page { children, page } => Control::Page {
                children: children.iter().filter_map(|r| child(self, r)).collect(),
                selected: (*page).max(0) as usize,
            },
            ElementKind::Radio { children, selected } => Control::Radio {
                children: children.iter().filter_map(|r| child(self, r)).collect(),
                selected: (*selected).max(0) as usize,
            },
            ElementKind::DocumentText(d) => Control::Document(self.load_document(d)),
            ElementKind::List {
                refs,
                sprite_sheet,
                sprite,
                ..
            } => {
                let f = refs[0]
                    .as_deref()
                    .and_then(|r| self.res.element(r))
                    .and_then(|e| match &e.kind {
                        ElementKind::Text(t) => t.font.clone(),
                        _ => None,
                    });
                let font = f.as_deref().and_then(|f| self.res.font(f));
                let highlight = self.res.sprite(sprite_sheet.as_deref(), sprite);
                let id = self.add(Node::new(Kind::List(ListData {
                    items: Vec::new(),
                    selected: None,
                    hovered: None,
                    scroll: 0,
                    font,
                    row_height: font.map_or(20, |f| self.res.fonts[f].font.size + 4),
                    highlight,
                    rows: Vec::new(),
                    reveal: false,
                })));
                Control::Clip {
                    content: Some(id),
                    size: [0, 0],
                    offset: [0, 0],
                }
            }
            _ => unreachable!("ordinary element handled by Ui"),
        }
    }

    pub fn set_value(&mut self, id: NodeId, value: f32) {
        if !value.is_finite() {
            return;
        }
        let mut label = None;
        match &mut self.nodes[id].kind {
            Kind::Control(Control::Slider(s)) => {
                s.value = value.clamp(s.range[0].min(s.range[1]), s.range[0].max(s.range[1]));
                label = s.number.map(|n| (n, format_number(s.value)));
            }
            Kind::Control(
                Control::Progress { value: v, .. } | Control::Scroll { value: v, .. },
            ) => *v = value.clamp(0.0, 1.0),
            Kind::Control(Control::Edit(e)) => {
                let value = if let Some(range) = e.range {
                    value
                        .clamp(range[0].min(range[1]), range[0].max(range[1]))
                        .trunc()
                } else {
                    value
                };
                e.text.text = format_number(value);
                e.cursor = e.text.text.chars().count();
            }
            _ => {}
        }
        if let Some((n, text)) = label {
            self.set_text(n, &text);
        }
    }
    pub fn value(&self, id: NodeId) -> Option<f32> {
        match &self.nodes[id].kind {
            Kind::Control(Control::Slider(s)) => Some(s.value),
            Kind::Control(Control::Progress { value, .. } | Control::Scroll { value, .. }) => {
                Some(*value)
            }
            Kind::Control(Control::Edit(e)) => e.text.text.parse().ok(),
            _ => None,
        }
    }
    pub fn set_slider_range(&mut self, id: NodeId, range: [f32; 2]) {
        if let Kind::Control(Control::Slider(slider)) = &mut self.nodes[id].kind {
            slider.range = range;
            slider.value = slider
                .value
                .clamp(range[0].min(range[1]), range[0].max(range[1]));
            slider.step = (range[1] - range[0]).abs() / 100.0;
        }
    }
    /// `UI::TableElement::SetChild(child, index)`: put `child` in cell `index` (row-major).
    pub fn set_table_child(&mut self, table: NodeId, index: usize, child: NodeId) {
        if let Kind::Control(Control::Table {
            cells,
            columns,
            rows,
            ..
        }) = &mut self.nodes[table].kind
        {
            let cols = columns.len().max(1);
            let (x, y) = (index % cols, index / cols);
            if y >= rows.len() {
                rows.resize(y + 1, 0);
            }
            cells.retain(|c| (c.0, c.1) != (x, y));
            cells.push((x, y, child));
        }
    }

    /// `UI::ImageElement::SetSprite` on every image inside `id`.
    pub fn set_images(&mut self, id: NodeId, sprite: SpriteRef) {
        let mut stack = vec![id];
        while let Some(n) = stack.pop() {
            if let Kind::Image { sprite: s, .. } = &mut self.nodes[n].kind {
                *s = Some(sprite);
            }
            stack.extend(self.child_ids(n));
        }
    }

    /// Whether a check control is checked.
    pub fn checked(&self, id: NodeId) -> bool {
        matches!(
            self.nodes[id].kind,
            Kind::Control(Control::Check { checked: true, .. })
        )
    }

    pub fn set_checked(&mut self, id: NodeId, value: bool) {
        if let Kind::Control(Control::Check { checked, .. }) = &mut self.nodes[id].kind {
            *checked = value;
        }
    }

    pub fn set_choices(&mut self, id: NodeId, choices: Vec<String>) {
        if let Kind::Control(Control::Combo { list, .. }) = self.nodes[id].kind {
            self.set_list_items(list, choices);
        }
    }
    pub fn set_graph_series(&mut self, id: NodeId, series: Vec<Vec<f32>>) {
        if let Kind::Control(Control::Graph { series: s, .. }) = &mut self.nodes[id].kind {
            *s = series;
        }
    }
    pub(crate) fn measure_control(&mut self, id: NodeId, mut c: Control) -> [i32; 2] {
        let mut max = [0, 0];
        for child in c.children() {
            let s = self.padded_size(child);
            max = [max[0].max(s[0]), max[1].max(s[1])];
        }
        let size = match &mut c {
            Control::Edit(e) => {
                self.layout_text(&mut e.text);
                [
                    e.text.layout.width.max(24),
                    e.text
                        .layout
                        .height
                        .max(e.text.font.map_or(16, |f| self.res.fonts[f].font.size)),
                ]
            }
            Control::Slider(s) => [
                s.sprites
                    .iter()
                    .take(3)
                    .flatten()
                    .map(|s| s.width)
                    .sum::<i32>()
                    .max(80)
                    + max[0],
                s.sprites
                    .iter()
                    .flatten()
                    .map(|s| s.height)
                    .max()
                    .unwrap_or(16)
                    .max(max[1]),
            ],
            Control::Scroll {
                sprites, vertical, ..
            } => {
                if *vertical {
                    [
                        sprites.first().and_then(|s| *s).map_or(12, |s| s.width),
                        sprites
                            .iter()
                            .take(3)
                            .flatten()
                            .map(|s| s.height)
                            .sum::<i32>()
                            .max(32),
                    ]
                } else {
                    [
                        sprites
                            .iter()
                            .take(3)
                            .flatten()
                            .map(|s| s.width)
                            .sum::<i32>()
                            .max(32),
                        sprites.first().and_then(|s| *s).map_or(12, |s| s.height),
                    ]
                }
            }
            Control::Progress { sprites, .. } => [
                max[0].max(sprites.iter().take(3).flatten().map(|s| s.width).sum()),
                max[1].max(
                    sprites
                        .iter()
                        .flatten()
                        .map(|s| s.height)
                        .max()
                        .unwrap_or(12),
                ),
            ],
            Control::Graph { size, .. } => *size,
            Control::Table {
                cells,
                columns,
                rows,
                spacing,
            } => {
                columns.fill(0);
                rows.fill(0);
                for &(x, y, n) in cells.iter() {
                    if self.nodes[n].visible() {
                        let s = self.padded_size(n);
                        columns[x] = columns[x].max(s[0]);
                        rows[y] = rows[y].max(s[1]);
                    }
                }
                [axis_size(columns, *spacing), axis_size(rows, *spacing)]
            }
            Control::Layout(items) => {
                let rects = self.relative_rects(items);
                bounds_size(&rects)
            }
            Control::Tabs {
                tabs,
                gap,
                header_height,
                ..
            } => {
                let mut w = 0;
                let mut h = 0;
                let mut body = [0, 0];
                for &(a, b) in tabs.iter() {
                    let s = self.padded_size(a);
                    w += s[0] + *gap;
                    h = h.max(s[1]);
                    let s = self.padded_size(b);
                    body = [body[0].max(s[0]), body[1].max(s[1])];
                }
                *header_height = h + 2 * *gap;
                [
                    w.max(body[0]) + 2 * *gap,
                    *header_height + body[1] + 2 * *gap,
                ]
            }
            Control::Clip { size, .. } | Control::Container { size, .. } => [
                if size[0] > 0 { size[0] } else { max[0] },
                if size[1] > 0 { size[1] } else { max[1] },
            ],
            Control::Spinner { parts, .. } => {
                let edit = parts[0].map_or([0, 0], |n| self.padded_size(n));
                let up = parts[1].map_or([0, 0], |n| self.padded_size(n));
                let down = parts[2].map_or([0, 0], |n| self.padded_size(n));
                [edit[0] + up[0].max(down[0]), edit[1].max(up[1] + down[1])]
            }
            Control::Document(d) => {
                self.layout_document(d);
                d.size
            }
            _ => max,
        };
        self.nodes[id].kind = Kind::Control(c);
        size
    }

    fn relative_rects(&mut self, items: &[LayoutChild]) -> Vec<[i32; 4]> {
        let sizes: Vec<_> = items.iter().map(|i| self.padded_size(i.node)).collect();
        let mut rects = vec![[0; 4]; items.len()];
        let mut state = vec![0u8; items.len()];
        fn resolve(
            i: usize,
            items: &[LayoutChild],
            sizes: &[[i32; 2]],
            rects: &mut [[i32; 4]],
            state: &mut [u8],
        ) {
            if state[i] != 0 {
                return;
            }
            state[i] = 1;
            let spec = &items[i].spec;
            let mut pos = [0, 0];
            for axis in 0..2 {
                let anchor = items.iter().position(|x| {
                    x.spec.strings[0] == spec.strings[axis + 1]
                        && !spec.strings[axis + 1].is_empty()
                });
                let (start, end) = if let Some(j) = anchor {
                    resolve(j, items, sizes, rects, state);
                    (rects[j][axis], rects[j][axis + 2])
                } else {
                    (0, 0)
                };
                pos[axis] = spec.unknown_i32[axis]
                    + match spec.unknown_u32[axis] {
                        1 => start,
                        2 => end,
                        3 => start - sizes[i][axis],
                        4 => end - sizes[i][axis],
                        5 => (start + end - sizes[i][axis]) / 2,
                        _ => 0,
                    };
            }
            rects[i] = [pos[0], pos[1], pos[0] + sizes[i][0], pos[1] + sizes[i][1]];
            state[i] = 2;
        }
        for i in 0..items.len() {
            resolve(i, items, &sizes, &mut rects, &mut state);
        }
        let minx = rects.iter().map(|r| r[0]).min().unwrap_or(0);
        let miny = rects.iter().map(|r| r[1]).min().unwrap_or(0);
        for r in &mut rects {
            r[0] -= minx;
            r[2] -= minx;
            r[1] -= miny;
            r[3] -= miny;
        }
        rects
    }

    pub(crate) fn layout_control(&mut self, id: NodeId) {
        let Kind::Control(mut c) = self.nodes[id].kind.clone() else {
            return;
        };
        let [l, t, r, b] = self.nodes[id].rect;
        let layout =
            |ui: &mut Ui, n: NodeId, rc: [i32; 4]| ui.compute_layout(n, rc[0], rc[1], rc[2], rc[3]);
        match &mut c {
            Control::Table {
                cells,
                columns,
                rows,
                spacing,
            } => {
                for &(x, y, n) in cells.iter() {
                    let px = l + axis_offset(columns, x, *spacing);
                    let py = t + axis_offset(rows, y, *spacing);
                    layout(self, n, [px, py, px + columns[x], py + rows[y]]);
                }
            }
            Control::Layout(items) => {
                for (item, rc) in items.iter().zip(self.relative_rects(items)) {
                    layout(
                        self,
                        item.node,
                        [l + rc[0], t + rc[1], l + rc[2], t + rc[3]],
                    );
                }
            }
            Control::Tabs {
                decoration,
                tabs,
                gap,
                header_height,
                selected,
            } => {
                for (i, n) in decoration.iter().enumerate() {
                    if let Some(n) = n {
                        layout(
                            self,
                            *n,
                            if i == 0 {
                                [l, t, r, t + *header_height]
                            } else {
                                [l, t + *header_height, r, b]
                            },
                        );
                    }
                }
                let mut x = l + *gap;
                for (a, _) in tabs.iter() {
                    let s = self.padded_size(*a);
                    layout(self, *a, [x, t + *gap, x + s[0], t + *header_height]);
                    x += s[0] + *gap;
                }
                for (i, &(header, _)) in tabs.iter().enumerate() {
                    if let Kind::Button { state, .. } = &mut self.nodes[header].kind {
                        *state = if i == *selected {
                            3
                        } else if self.hot == Some(header) {
                            2
                        } else {
                            0
                        };
                    }
                }
                if let Some((_, n)) = tabs.get(*selected) {
                    layout(
                        self,
                        *n,
                        [l + *gap, t + *header_height + *gap, r - *gap, b - *gap],
                    );
                }
            }
            Control::Container {
                content,
                bars,
                spacing,
                offset,
                view,
                ..
            } => {
                let content_size = content.map_or([0, 0], |n| self.padded_size(n));
                let mut need = [false, false];
                for _ in 0..2 {
                    let w = r
                        - l
                        - if need[1] {
                            bars[1].map_or(0, |n| self.nodes[n].size[0]) + *spacing
                        } else {
                            0
                        };
                    let h = b
                        - t
                        - if need[0] {
                            bars[0].map_or(0, |n| self.nodes[n].size[1]) + *spacing
                        } else {
                            0
                        };
                    need = [
                        bars[0].is_some() && content_size[0] > w,
                        bars[1].is_some() && content_size[1] > h,
                    ];
                }
                let vr = r - if need[1] {
                    bars[1].map_or(0, |n| self.nodes[n].size[0]) + *spacing
                } else {
                    0
                };
                let vb = b - if need[0] {
                    bars[0].map_or(0, |n| self.nodes[n].size[1]) + *spacing
                } else {
                    0
                };
                *view = [l, t, vr.max(l), vb.max(t)];
                for axis in 0..2 {
                    let extent = (view[axis + 2] - view[axis]).max(0);
                    let max_scroll = (content_size[axis] - extent).max(0);
                    offset[axis] = offset[axis].clamp(0, max_scroll);
                    if let Some(bar) = bars[axis] {
                        if let Kind::Control(Control::Scroll { value, page, .. }) =
                            &mut self.nodes[bar].kind
                        {
                            *value = if max_scroll > 0 {
                                offset[axis] as f32 / max_scroll as f32
                            } else {
                                0.0
                            };
                            *page = if content_size[axis] > 0 {
                                (extent as f32 / content_size[axis] as f32).clamp(0.0, 1.0)
                            } else {
                                1.0
                            };
                        }
                        layout(
                            self,
                            bar,
                            if need[axis] {
                                if axis == 0 {
                                    [l, vb + *spacing, vr, b]
                                } else {
                                    [vr + *spacing, t, r, vb]
                                }
                            } else {
                                [0; 4]
                            },
                        );
                    }
                }
                if let Some(n) = content {
                    layout(
                        self,
                        *n,
                        [
                            l - offset[0],
                            t - offset[1],
                            l - offset[0] + content_size[0].max(vr - l),
                            t - offset[1] + content_size[1].max(vb - t),
                        ],
                    );
                }
            }
            Control::Clip {
                content, offset, ..
            } => {
                if let Some(n) = content {
                    let size = self.nodes[*n].size;
                    layout(
                        self,
                        *n,
                        [
                            l - offset[0],
                            t - offset[1],
                            r.max(l + size[0]) - offset[0],
                            b.max(t + size[1]) - offset[1],
                        ],
                    );
                }
            }
            Control::Combo { button, list, .. } => {
                layout(self, *button, [l, t, r, b]);
                self.nodes[*button].rect = [l, t, r, b];
                let height = match &self.nodes[*list].kind {
                    Kind::List(l) => {
                        (l.items.len().min(10) as i32 * l.row_height).max(l.row_height)
                    }
                    _ => 20,
                };
                let top = if b + height > self.screen[1] {
                    (t - height).max(0)
                } else {
                    b
                };
                layout(self, *list, [l, top, r, top + height]);
                self.nodes[*list].rect = [l, top, r, top + height];
            }
            Control::Slider(s) => {
                if let Some(n) = s.number {
                    let width = self.padded_size(n)[0];
                    let knob = s.sprites.get(3).and_then(|sp| *sp).map_or(0, |sp| sp.width);
                    let x = l
                        + s.inset
                        + ((r - l - 2 * s.inset - knob).max(0) as f32 * fraction(s.value, s.range))
                            as i32;
                    let tx = if x + knob + width < r {
                        x + knob + s.inset
                    } else {
                        x - width
                    };
                    layout(self, n, [tx, t + s.track_y, tx + width, b]);
                }
            }
            Control::Spinner { parts, .. } => {
                let width = parts[0].map_or(0, |n| self.padded_size(n)[0]);
                let mid = t + (b - t) / 2;
                if let Some(n) = parts[0] {
                    layout(self, n, [l, t, l + width, b]);
                }
                if let Some(n) = parts[1] {
                    layout(self, n, [l + width, t, r, mid]);
                }
                if let Some(n) = parts[2] {
                    layout(self, n, [l + width, mid, r, b]);
                }
            }
            Control::Edit(e) => {
                self.layout_text(&mut e.text);
                let prefix = e.text.text.chars().take(e.cursor).collect::<String>();
                let width = e.text.font.map_or(0, |f| {
                    TextLayout::compute(&self.res, f, &prefix, None, 0).width
                });
                e.scroll = e.scroll.min(width).max(width - (r - l - 8).max(1));
            }
            Control::Document(d) => {
                d.width = (r - l).max(1);
                self.layout_document(d);
            }
            Control::Radio { children, selected } => {
                for (i, &n) in children.iter().enumerate() {
                    if let Kind::Control(Control::Check { checked, .. }) = &mut self.nodes[n].kind {
                        *checked = i == *selected;
                    }
                    layout(self, n, [l, t, r, b]);
                }
            }
            _ => {
                for n in c.active_children() {
                    layout(self, n, [l, t, r, b]);
                }
            }
        }
        self.nodes[id].kind = Kind::Control(c);
    }

    pub(crate) fn draw_control(
        &self,
        id: NodeId,
        c: &Control,
        color: u32,
        clip: Option<[i32; 4]>,
        out: &mut Vec<Quad>,
    ) {
        let rc = self.nodes[id].rect;
        let [l, t, r, b] = rc;
        let solid = self.res.sheets.iter().enumerate().find_map(|(sheet, s)| {
            s.sheet.sprite("White").map(|sp| SpriteRef {
                sheet,
                width: sp.width,
                height: sp.height,
                uv: sp.uv,
            })
        });
        match c {
            Control::Slider(s) => {
                let track_sprites = &s.sprites[..3];
                let knob_sprite = s
                    .sprites
                    .get(if self.hot == Some(id) { 4 } else { 3 })
                    .and_then(|s| *s);
                if track_sprites.iter().any(Option::is_some) {
                    strip(
                        out,
                        track_sprites,
                        [
                            l,
                            t + s.track_y,
                            r,
                            t + s.track_y
                                + s.sprites.get(1).and_then(|s| *s).map_or(4, |s| s.height),
                        ],
                        color,
                        clip,
                        false,
                    );
                }
                let f = fraction(s.value, s.range);
                if let Some(sp) = knob_sprite {
                    let x =
                        l + s.inset + ((r - l - 2 * s.inset - sp.width).max(0) as f32 * f) as i32;
                    Self::sprite_quad(out, &sp, [x, t, x + sp.width, t + sp.height], color, clip);
                } else if let Some(solid) = solid {
                    let track = [
                        l + s.inset,
                        t + (b - t) / 2 - 3,
                        r - s.inset,
                        t + (b - t) / 2 + 3,
                    ];
                    Self::sprite_quad(out, &solid, track, modulate(0xFF49453F, color), clip);
                    let fill = [
                        track[0],
                        track[1],
                        track[0] + ((track[2] - track[0]) as f32 * f) as i32,
                        track[3],
                    ];
                    Self::sprite_quad(out, &solid, fill, modulate(0xFFC7AF61, color), clip);
                    let x = l + s.inset + ((r - l - 2 * s.inset - 14).max(0) as f32 * f) as i32;
                    let y = t + ((b - t - 18).max(0) / 2);
                    Self::sprite_quad(
                        out,
                        &solid,
                        [x, y, x + 14, y + 18],
                        modulate(0xFFE9E0C8, color),
                        clip,
                    );
                }
            }
            Control::Progress {
                sprites,
                background_color,
                value,
                ..
            } => {
                strip(
                    out,
                    &sprites[3..],
                    rc,
                    modulate(*background_color, color),
                    clip,
                    false,
                );
                if *value > 0.0 {
                    let fill = [l, t, l + ((r - l) as f32 * value.clamp(0.0, 1.0)) as i32, b];
                    strip(
                        out,
                        &sprites[..3],
                        fill,
                        color,
                        intersect(clip, fill),
                        false,
                    );
                }
            }
            Control::Scroll {
                sprites,
                vertical,
                value,
                page,
            } => {
                strip(out, &sprites[..3], rc, color, clip, *vertical);
                let thumb = scroll_thumb(rc, *vertical, *value, *page);
                let start = if self.hot == Some(id) { 6 } else { 3 };
                strip(
                    out,
                    &sprites[start..start + 3],
                    thumb,
                    color,
                    intersect(clip, rc),
                    *vertical,
                );
            }
            Control::Edit(e) => {
                let clip = intersect(clip, rc);
                // Edit boxes inset their text 3px; display-only numbers (`NumberElement`) draw
                // in their own rectangle.
                let inset = if e.max_length > 0 { 3 } else { 0 };
                self.draw_text(&e.text, [l + inset - e.scroll, t], color, clip, out);
                if self.focus == Some(id) && e.max_length > 0 {
                    if let Some(sp) = solid {
                        let prefix = e.text.text.chars().take(e.cursor).collect::<String>();
                        let x = l + 3 - e.scroll
                            + e.text.font.map_or(0, |f| {
                                TextLayout::compute(&self.res, f, &prefix, None, 0).width
                            });
                        Self::sprite_quad(out, &sp, [x, t, x + 1, b], color, clip);
                    }
                }
            }
            Control::Graph {
                sprite,
                series,
                color: line_color,
                ..
            } => {
                if let Some(sp) = sprite.or(solid) {
                    let clip = intersect(clip, rc);
                    for i in 0..5 {
                        let y = t + (b - t - 1) * i / 4;
                        Self::sprite_quad(
                            out,
                            &sp,
                            [l, y, r, y + 1],
                            modulate(color, 0x40505050),
                            clip,
                        );
                    }
                    let finite: Vec<_> = series
                        .iter()
                        .flatten()
                        .filter(|x| x.is_finite())
                        .copied()
                        .collect();
                    let min = finite.iter().copied().fold(0.0, f32::min);
                    let max = finite.iter().copied().fold(1.0, f32::max);
                    for (j, points) in series.iter().enumerate() {
                        let tint = if *line_color == 0 {
                            [0xFFF6CD51, 0xFF80C8FF, 0xFF80D080][j % 3]
                        } else {
                            *line_color
                        };
                        for (i, pair) in points.windows(2).enumerate() {
                            if !pair[0].is_finite() || !pair[1].is_finite() {
                                continue;
                            }
                            let x0 = l + (r - l - 1) * i as i32 / (points.len() - 1).max(1) as i32;
                            let x1 =
                                l + (r - l - 1) * (i + 1) as i32 / (points.len() - 1).max(1) as i32;
                            let y0 =
                                b - 1 - ((pair[0] - min) / (max - min) * (b - t - 1) as f32) as i32;
                            let y1 =
                                b - 1 - ((pair[1] - min) / (max - min) * (b - t - 1) as f32) as i32;
                            let steps = (x1 - x0).abs().max((y1 - y0).abs()).max(1);
                            for k in 0..=steps {
                                let x = x0 + (x1 - x0) * k / steps;
                                let y = y0 + (y1 - y0) * k / steps;
                                Self::sprite_quad(
                                    out,
                                    &sp,
                                    [x, y, x + 1, y + 1],
                                    modulate(color, tint),
                                    clip,
                                );
                            }
                        }
                    }
                }
            }
            Control::Clip { content, .. } => {
                if let Some(n) = content {
                    self.draw_node(*n, color, intersect(clip, rc), out);
                }
                return;
            }
            Control::Container {
                content,
                bars,
                view,
                ..
            } => {
                if let Some(n) = content {
                    self.draw_node(*n, color, intersect(clip, *view), out);
                }
                for n in bars.iter().flatten() {
                    self.draw_node(*n, color, clip, out);
                }
                return;
            }
            Control::Document(d) => {
                self.draw_document(d, [l, t], color, intersect(clip, rc), out);
            }
            Control::Combo { button, list, open } => {
                self.draw_node(*button, color, clip, out);
                if *open {
                    if let Some(sp) = solid {
                        Self::sprite_quad(out, &sp, self.nodes[*list].rect, 0xFF30281F, None);
                    }
                    self.draw_node(*list, color, None, out);
                }
                return;
            }
            _ => {}
        }
        for child in c.active_children() {
            self.draw_node(child, color, clip, out);
        }
    }
}

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
fn axis_size(v: &[i32], gap: i32) -> i32 {
    v.iter().sum::<i32>() + (v.iter().filter(|&&v| v > 0).count() as i32 - 1).max(0) * gap
}
fn axis_offset(v: &[i32], i: usize, gap: i32) -> i32 {
    v[..i].iter().sum::<i32>() + v[..i].iter().filter(|&&v| v > 0).count() as i32 * gap
}
fn bounds_size(rects: &[[i32; 4]]) -> [i32; 2] {
    [
        rects.iter().map(|r| r[2]).max().unwrap_or(0),
        rects.iter().map(|r| r[3]).max().unwrap_or(0),
    ]
}
pub(crate) fn scroll_thumb(rc: [i32; 4], vertical: bool, value: f32, page: f32) -> [i32; 4] {
    let axis = if vertical { 1 } else { 0 };
    let length = (rc[axis + 2] - rc[axis]).max(0);
    let size = ((length as f32 * page.clamp(0.0, 1.0)) as i32)
        .max(12)
        .min(length);
    let start = rc[axis] + ((length - size) as f32 * value.clamp(0.0, 1.0)) as i32;
    let mut out = rc;
    out[axis] = start;
    out[axis + 2] = start + size;
    out
}
fn strip(
    out: &mut Vec<Quad>,
    s: &[Option<SpriteRef>],
    rc: [i32; 4],
    color: u32,
    clip: Option<[i32; 4]>,
    vertical: bool,
) {
    let axis = if vertical { 1 } else { 0 };
    let length = (rc[axis + 2] - rc[axis]).max(0);
    let size = |i: usize| {
        s.get(i)
            .and_then(|s| *s)
            .map_or(0, |s| if vertical { s.height } else { s.width })
    };
    let a = size(0).min(length);
    let b = size(2).min(length - a);
    let mut cells = [rc; 3];
    cells[0][axis + 2] = rc[axis] + a;
    cells[1][axis] = rc[axis] + a;
    cells[1][axis + 2] = rc[axis + 2] - b;
    cells[2][axis] = rc[axis + 2] - b;
    for (s, r) in s.iter().take(3).zip(cells) {
        if let Some(s) = s {
            Ui::sprite_quad(out, s, r, color, clip);
        }
    }
}
