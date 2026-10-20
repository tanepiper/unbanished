//! Making elements from the game's dialog resources: one node per element, with its text, sprites
//! and children.

use super::*;

impl Ui {
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

    pub(super) fn instantiate_reference(&mut self, reference: &str) -> Option<NodeId> {
        let element = self.res.element(reference)?;
        let name = reference
            .rsplit_once(':')
            .map_or(reference, |(_, n)| n)
            .to_string();
        let id = self.instantiate_element(&element, name);
        self.nodes[id].reference = reference.to_string();
        Some(id)
    }

    pub(super) fn text_data(&mut self, t: &banished_formats::ui::Text) -> TextData {
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

    pub(super) fn instantiate_element(&mut self, e: &Element, name: String) -> NodeId {
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
                    clock: 0.0,
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
                icon_color: 0xFFFF_FFFF,
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
        node.cursor = b.cursor;
        node.children = children;
        node.tooltip = self.res.string(b.string_table.as_deref(), &b.tooltip);
        node.tooltip_dialog = b.tooltip_dialog.clone();
        node.string_table = b.string_table.clone();
        node.border = child(self, &b.border);

        self.add(node)
    }
}
