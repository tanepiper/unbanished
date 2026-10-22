//! Tooltips: which element shows one, placing it, its text's parameters and drawing it.

use super::*;

impl Ui {
    pub(super) fn update_states(&mut self, _root: NodeId) {
        for id in 0..self.nodes.len() {
            let hot = self.hot == Some(id) || self.focus == Some(id);
            let pressed = self.pressed == Some(id) && self.pointer_down;
            let enabled = self.nodes[id].usable();
            if let Kind::Button { state, .. } = &mut self.nodes[id].kind {
                *state = if !enabled {
                    1
                } else if hot && pressed {
                    3
                } else if hot {
                    2
                } else {
                    0
                };
            }
            let button = match &self.nodes[id].kind {
                Kind::Control(Control::Check { button, .. } | Control::Combo { button, .. }) => {
                    Some(*button)
                }
                _ => None,
            };
            if let Some(button) = button {
                if let Kind::Button { state, .. } = &mut self.nodes[button].kind {
                    *state = if !enabled {
                        1
                    } else if hot && pressed {
                        3
                    } else if hot {
                        2
                    } else {
                        0
                    };
                }
            }
        }
    }

    /// The tooltip text of the element under the pointer.
    pub fn hovered_tooltip(&self) -> Option<&str> {
        self.hot
            .map(|h| self.nodes[h].tooltip.as_str())
            .filter(|t| !t.is_empty())
    }

    /// The element whose tooltip shows: the topmost element under the pointer (an open
    /// dropdown list first, else the last drawn) or the nearest of its ancestors with tooltip
    /// text. Elements covered by a later dialog do not show theirs.
    pub(super) fn tooltip_source(&self, root: NodeId) -> Option<NodeId> {
        let (x, y) = self.pointer;
        let under = |id: NodeId| {
            let n = &self.nodes[id];
            n.contains(x, y) && n.rect[2] > n.rect[0] && n.rect[3] > n.rect[1]
        };
        let popup = self.open_lists(root).into_iter().rev().find(|&l| under(l));
        let nodes: Vec<NodeId> = match popup {
            Some(l) => self.active_nodes(l),
            None => self.active_nodes(root),
        };
        let under: Vec<NodeId> = nodes.into_iter().filter(|&id| under(id)).collect();
        let top = *under.last()?;
        under
            .into_iter()
            .rev()
            .find(|&id| !self.nodes[id].tooltip.is_empty() && self.descendant(id, top))
    }

    pub(super) fn update_tooltip(&mut self, root: NodeId) {
        let Some(source) = self.tooltip_source(root) else {
            return;
        };
        // `UserInterface::UpdateToolTip` (0x180123300): the open tooltip follows the pointer.
        if self.tooltip_instance.is_some_and(|(s, _)| s == source) {
            self.place_tooltip();
            return;
        }
        // `Element::IsToolTipValid` (0x180036140): a tooltip needs the element's own tooltip
        // dialog as well as its text.
        let Some(reference) = self.nodes[source].tooltip_dialog.clone() else {
            return;
        };
        if let Some(dialog) = self.instantiate_dialog(&reference) {
            let text = self.nodes[source].tooltip.clone();
            for name in ["textToolTip", "text", "textDescription"] {
                if let Some(id) = self.find(dialog.root, name) {
                    self.set_parameters(id, &[(0, &text)]);
                    break;
                }
            }
            let controllers: Vec<u32> = dialog.bindings.iter().map(|b| b.controller).collect();
            // `HotKeyToolTipUI` (`FUN_14006b2e0`): with a key binding, its name is parameter 1
            // of `textToolTip` and parameter 0 of `textHotKey`.
            if controllers.contains(&tooltip_controller::HOT_KEY) {
                if let Some(key) = self.nodes[source].hotkey.clone() {
                    if let Some(id) = self.find(dialog.root, "textToolTip") {
                        self.set_parameters(id, &[(1, &key)]);
                    }
                    if let Some(id) = self.find(dialog.root, "textHotKey") {
                        self.set_parameters(id, &[(0, &key)]);
                    }
                }
            }
            // `TitleDescToolTipUI` (`FUN_140080c00`): the element's title and description.
            if controllers.contains(&tooltip_controller::TITLE_DESC) {
                if let Some((title, desc)) = self.nodes[source].title_desc.clone() {
                    for (name, value) in [("textTitle", title), ("textDesc", desc)] {
                        if let Some(id) = self.find(dialog.root, name) {
                            self.set_parameters(id, &[(0, &value)]);
                        }
                    }
                }
            }
            self.tooltip_instance = Some((source, dialog.root));
            self.tooltip_opened = Some((source, dialog.root, controllers));
            self.place_tooltip();
        }
    }

    /// Size and place the open tooltip dialog on the pointer (again after the application
    /// filled it).
    pub fn place_tooltip(&mut self) {
        let Some((_, root)) = self.tooltip_instance else {
            return;
        };
        self.begin_size_pass();
        self.compute_size(root);
        let [w, h] = self.nodes[root].size;
        // `BeginToolTip` / `UpdateToolTip`: `SetUpperLeft(x, y − height, false)`, the tooltip
        // standing on the pointer.
        let (x, y) = (self.pointer.0, self.pointer.1 - h);
        self.layout_dialog(root, [x, y, x + w, y + h]);
    }

    /// The tooltip dialog opened since the last call: source element, dialog root and the
    /// dialog's controller ids. Application controllers fill it, then call `place_tooltip`.
    pub fn new_tooltip(&mut self) -> Option<(NodeId, NodeId, Vec<u32>)> {
        self.tooltip_opened.take()
    }

    /// `UI::TextElement::SetParameter` on a text or label whose text still holds its template.
    pub fn set_parameters(&mut self, id: NodeId, values: &[(usize, &str)]) {
        let current = match &self.nodes[id].kind {
            Kind::Text(t) | Kind::Label { text: t, .. } => t.text.clone(),
            _ => return,
        };
        let mut out = current;
        for (i, v) in values {
            out = out.replace(&format!("@{i}"), v);
        }
        self.set_text(id, &out);
    }

    /// The open tooltip dialog, while its element is still the tooltip source under the
    /// pointer within the input root (a modal dialog hides the tooltips of what it covers).
    pub(crate) fn draw_tooltip(&self, root: NodeId, out: &mut Vec<Quad>) {
        if self.pointer_down {
            return;
        }
        let Some((source, tip)) = self.tooltip_instance else {
            return;
        };
        let input = self.input_root.unwrap_or(root);
        if tip < self.nodes.len() && self.tooltip_source(input) == Some(source) {
            self.draw_node(tip, 0xFFFFFFFF, None, out);
        }
    }
}
