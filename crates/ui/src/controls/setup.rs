//! Making controls from their resource descriptions (`instantiate_control`), and tabs added in
//! code.

use super::*;

impl Ui {
    /// Runtime UI::TabElement::AddTab: retain header/content order and select the first tab.
    pub fn add_tab(&mut self, parent: NodeId, header: NodeId, content: NodeId) {
        if let Kind::Control(Control::Tabs { tabs, .. }) = &mut self.nodes[parent].kind {
            tabs.push((header, content));
        }
    }

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

    pub(super) fn control_button(&mut self, b: &ui::Button) -> NodeId {
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
                steps: *unknown_b0,
                inset: unknown[1],
                track_y: unknown[0],
                number: child(self, track),
                over_knob: false,
                drag: None,
            }),
            ElementKind::Scroll {
                sprite_sheet,
                sprites: names,
                vertical,
                unknown,
                unknown_u32,
            } => Control::Scroll {
                sprites: sprites(self, sprite_sheet, names),
                vertical: *vertical,
                value: 0.0,
                page: 0.25,
                inset: *unknown,
                thumb_color: unknown_u32[0],
                drag: None,
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
                unknown_bool,
                unknown_string,
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
                password: *unknown_bool,
                allowed: unknown_string.clone(),
                caret: true,
                blink: 0.0,
            }),
            ElementKind::NumberEdit {
                font,
                unknown,
                unknown_bool,
            } => Control::Edit(EditData {
                text: self.make_text(font.as_deref(), unknown[0].to_string()),
                cursor: 1,
                max_length: unknown[3].max(1) as usize,
                numeric: true,
                range: Some([unknown[1] as f32, unknown[2] as f32]),
                password: false,
                allowed: if *unknown_bool {
                    "0123456789-".into()
                } else {
                    "0123456789".into()
                },
                caret: true,
                blink: 0.0,
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
                password: false,
                allowed: String::new(),
                caret: true,
                blink: 0.0,
            }),
            ElementKind::Graph {
                unknown,
                unknown_u32,
                sprite_sheet,
                sprite,
            } => Control::Graph {
                // `GraphDescription::CreateInstance`: size `+0xc0`, tick `+0xc8` (and an unused
                // colour, `+0xcc`); `GraphElement::GraphElement`: range 1, 12 by 4 divisions.
                size: [unknown[0], unknown[1]],
                sprite: self.res.sprite(sprite_sheet.as_deref(), sprite),
                tick: {
                    let _ = unknown_u32;
                    unknown[2]
                },
                y_range: 1.0,
                x_divisions: 12,
                y_divisions: 4,
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
                wheel: 0,
                fine: 0.0,
            },
            ElementKind::Check { button, check } => Control::Check {
                button: self.control_button(button),
                mark: child(self, check),
                checked: false,
            },
            ElementKind::Combo { button, refs } => {
                let b = self.control_button(button);
                // ComboElement derives from ButtonElement: the button is the whole combo.
                self.nodes[b].flags |= crate::flags::FILL_WIDTH | crate::flags::FILL_HEIGHT;
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
                    icons: Vec::new(),
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
}
