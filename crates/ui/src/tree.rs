//! Finding nodes in the tree, changing their text, icons and visibility, and building nodes in code
//! (groups, ribbons, text, lists, sliders).

use super::*;

impl Ui {
    /// Depth-first search for a node by element name below `root`.
    pub fn find(&self, root: NodeId, name: &str) -> Option<NodeId> {
        if self.nodes[root].name == name {
            return Some(root);
        }
        self.child_ids(root)
            .into_iter()
            .find_map(|c| self.find(c, name))
    }

    pub(super) fn child_ids(&self, id: NodeId) -> Vec<NodeId> {
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

    /// `LabelElement::SetSprite` on a label, or on a combo's label (`ComboElement::UpdateText`
    /// sets the chosen item's sprite when it has one).
    pub fn set_icon(&mut self, id: NodeId, sprite: SpriteRef) {
        let target = match &self.nodes[id].kind {
            Kind::Control(Control::Combo { button, .. }) => *button,
            _ => id,
        };
        let mut stack = vec![target];
        while let Some(n) = stack.pop() {
            if let Kind::Label { icon, .. } = &mut self.nodes[n].kind {
                *icon = Some(sprite);
                return;
            }
            stack.extend(self.nodes[n].children.iter().rev());
        }
    }

    /// A combo's item `index`: the list's selection, the label's text and the item's sprite.
    pub fn select_combo_item(&mut self, combo: NodeId, index: usize) {
        let Kind::Control(Control::Combo { list, .. }) = self.nodes[combo].kind else {
            return;
        };
        let (text, icon) = match &self.nodes[list].kind {
            Kind::List(l) => (
                l.items.get(index).cloned(),
                l.icons.get(index).copied().flatten(),
            ),
            _ => (None, None),
        };
        let Some(text) = text else { return };
        self.list_select(list, Some(index));
        self.set_text(combo, &text);
        if let Some(icon) = icon {
            self.set_icon(combo, icon);
        }
    }

    /// Show or hide an element.
    pub fn set_visible(&mut self, id: NodeId, visible: bool) {
        let f = &mut self.nodes[id].flags;
        *f = if visible {
            *f & !flags::HIDDEN
        } else {
            *f | flags::HIDDEN
        };
    }

    /// Add an empty group (built in code, not from a resource).
    pub fn group(&mut self) -> NodeId {
        self.add(Node::new(Kind::Group))
    }

    /// Add an empty ribbon: a row, or a column when `vertical`.
    pub fn ribbon(&mut self, vertical: bool, spacing: i32) -> NodeId {
        self.add(Node::new(Kind::Ribbon {
            vertical,
            uniform: false,
            spacing,
            cells: Vec::new(),
        }))
    }

    /// Add a text element with a font reference and a text.
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

    /// Add a viewport: space that the application draws into.
    pub fn viewport(&mut self) -> NodeId {
        self.add(Node::new(Kind::Viewport))
    }

    /// Add a picture: an application texture, drawn letterboxed.
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
            icons: Vec::new(),
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
            steps: 0,
            inset: 8,
            track_y: 0,
            number: None,
            over_knob: false,
            drag: None,
        }))))
    }

    /// Set the rows of a list.
    pub fn set_list_items(&mut self, id: NodeId, items: Vec<String>) {
        self.set_list_icons(id, items, Vec::new());
    }

    /// [`Self::set_list_items`] with each item's sprite.
    pub fn set_list_icons(
        &mut self,
        id: NodeId,
        items: Vec<String>,
        icons: Vec<Option<SpriteRef>>,
    ) {
        if let Kind::List(l) = &mut self.nodes[id].kind {
            l.icons = icons;
            l.items = items;
            l.selected = None;
            l.hovered = None;
            l.scroll = 0;
            l.rows.clear();
        }
    }

    /// Select a row of a list, or none.
    pub fn list_select(&mut self, id: NodeId, index: Option<usize>) {
        if let Kind::List(l) = &mut self.nodes[id].kind {
            l.selected = index.filter(|&i| i < l.items.len());
            l.reveal = true;
        }
    }

    pub(super) fn reveal_selection(&mut self, id: NodeId) {
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
}
