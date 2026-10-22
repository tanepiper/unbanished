//! The keyboard: keys and shortcuts, typing into an edit, committing an edit and the caret.

use super::*;

pub(super) fn byte_index(s: &str, cursor: usize) -> usize {
    s.char_indices().nth(cursor).map_or(s.len(), |p| p.0)
}

/// `System::String::ToInteger`: an optional sign and the digits after it, 0 without.
pub(super) fn to_integer(s: &str) -> i32 {
    let s = s.trim_start();
    let (sign, digits) = match s.strip_prefix('-') {
        Some(rest) => (-1i64, rest),
        None => (1, s.strip_prefix('+').unwrap_or(s)),
    };
    let mut v: i64 = 0;
    for c in digits.chars().take_while(|c| c.is_ascii_digit()) {
        v = (v * 10 + c as i64 - '0' as i64).min(i32::MAX as i64 + 1);
    }
    (sign * v).clamp(i32::MIN as i64, i32::MAX as i64) as i32
}

impl Ui {
    /// Press a key. Returns whether the interface used it.
    pub fn key(&mut self, root: NodeId, key: Key) -> bool {
        let active = self.active_nodes(root);
        if self.focus.is_some_and(|id| !active.contains(&id)) {
            self.focus = None;
        }
        if matches!(key, Key::Tab | Key::BackTab) {
            let focusable: Vec<_> = active
                .iter()
                .copied()
                .filter(|&id| self.interactive(id))
                .collect();
            if focusable.is_empty() {
                return false;
            }
            let current = self
                .focus
                .and_then(|id| focusable.iter().position(|&n| n == id));
            let i = match (current, key) {
                (Some(i), Key::BackTab) => (i + focusable.len() - 1) % focusable.len(),
                (Some(i), _) => (i + 1) % focusable.len(),
                (None, Key::BackTab) => focusable.len() - 1,
                _ => 0,
            };
            self.focus = Some(focusable[i]);
            return true;
        }
        if let Some(id) = self.focus {
            // A combo's popup retains focus while arrows browse; Enter confirms and Escape
            // closes it. These keys must not fall through to the asset browser's navigation.
            let combo = active
                .iter()
                .find_map(|&parent| match self.nodes[parent].kind {
                    Kind::Control(Control::Combo { list, .. }) if parent == id || list == id => {
                        Some((parent, list))
                    }
                    _ => None,
                });
            if let Some((parent, list)) = combo {
                if matches!(key, Key::Up | Key::Down | Key::Home | Key::End) {
                    if let Kind::Control(Control::Combo { open, .. }) = &mut self.nodes[parent].kind
                    {
                        *open = true;
                    }
                    if let Kind::List(l) = &mut self.nodes[list].kind {
                        if !l.items.is_empty() {
                            let last = l.items.len() - 1;
                            let i = match key {
                                Key::Home => 0,
                                Key::End => last,
                                Key::Up => l.selected.unwrap_or(0).saturating_sub(1),
                                _ => l.selected.map_or(0, |i| (i + 1).min(last)),
                            };
                            l.selected = Some(i);
                            l.reveal = true;
                        }
                    }
                    self.focus = Some(list);
                    return true;
                }
                if key == Key::Escape {
                    if let Kind::Control(Control::Combo { open, .. }) = &mut self.nodes[parent].kind
                    {
                        *open = false;
                    }
                    self.focus = Some(parent);
                    return true;
                }
                if id == list && matches!(key, Key::Enter | Key::Space) {
                    if let Kind::List(l) = &self.nodes[list].kind {
                        if let Some(i) = l.selected {
                            let event = self.route_event(root, Event::Selected(list, i));
                            self.pending.push(event);
                        }
                    }
                    self.focus = Some(parent);
                    return true;
                }
            }
            if let Kind::Control(Control::Edit(e)) = &mut self.nodes[id].kind {
                let len = e.text.text.chars().count();
                e.cursor = e.cursor.min(len);
                let mut changed = false;
                match key {
                    Key::Left => e.cursor = e.cursor.saturating_sub(1),
                    Key::Right => e.cursor = (e.cursor + 1).min(len),
                    Key::Home => e.cursor = 0,
                    Key::End => e.cursor = len,
                    Key::Backspace if e.cursor > 0 => {
                        let at = byte_index(&e.text.text, e.cursor);
                        let prev = byte_index(&e.text.text, e.cursor - 1);
                        e.text.text.replace_range(prev..at, "");
                        e.cursor -= 1;
                        changed = true;
                    }
                    Key::Delete if e.cursor < len => {
                        let at = byte_index(&e.text.text, e.cursor);
                        let next = byte_index(&e.text.text, e.cursor + 1);
                        e.text.text.replace_range(at..next, "");
                        changed = true;
                    }
                    // `EditElement::Update`: Enter or Escape gives up the focus.
                    Key::Enter | Key::Escape => {
                        self.commit_edit(id);
                        return true;
                    }
                    Key::Character(_) | Key::Space => return false,
                    _ => return true,
                }
                if changed {
                    self.pending.push(Event::Changed(id));
                }
                return true;
            }
            if let Kind::Control(Control::Slider(s)) = &self.nodes[id].kind {
                let step = s.step();
                let value = match key {
                    Key::Left | Key::Down => s.value - step,
                    Key::Right | Key::Up => s.value + step,
                    Key::Home => s.range[0],
                    Key::End => s.range[1],
                    _ => f32::NAN,
                };
                if value.is_finite() {
                    self.set_value(id, value);
                    self.pending.push(Event::Changed(id));
                    return true;
                }
            }
            if let Kind::List(l) = &mut self.nodes[id].kind {
                let last = l.items.len().saturating_sub(1);
                let current = l.selected.unwrap_or(0);
                let selected = match key {
                    Key::Up => Some(current.saturating_sub(1)),
                    Key::Down => Some(l.selected.map_or(0, |i| (i + 1).min(last))),
                    Key::Home => Some(0),
                    Key::End => Some(last),
                    Key::PageUp => Some(current.saturating_sub(10)),
                    Key::PageDown => Some((current + 10).min(last)),
                    _ => None,
                };
                if let Some(i) = selected.filter(|_| !l.items.is_empty()) {
                    l.selected = Some(i);
                    l.reveal = true;
                    let e = self.route_event(root, Event::Selected(id, i));
                    self.pending.push(e);
                    return true;
                }
            }
            if matches!(key, Key::Enter | Key::Space) {
                if let Some(event) = self.activate(id) {
                    let event = self.route_event(root, event);
                    self.pending.push(event);
                    return true;
                }
            }
        }
        if key == Key::Escape {
            let mut closed = false;
            for id in &active {
                if let Kind::Control(Control::Combo { open, .. }) = &mut self.nodes[*id].kind {
                    closed |= *open;
                    *open = false;
                }
            }
            if closed {
                return true;
            }
        }
        let code = match key {
            Key::Enter => 13,
            Key::Escape => 27,
            Key::Space => 32,
            Key::Character(c) => c.to_ascii_uppercase() as u32,
            _ => 0,
        };
        if code != 0 {
            if let Some(id) = active
                .into_iter()
                .find(|&id| self.nodes[id].shortcut == Some(code))
            {
                if let Some(e) = self.activate(id) {
                    let e = self.route_event(root, e);
                    self.pending.push(e);
                    return true;
                }
            }
        }
        false
    }

    /// Type text into the focused edit box. Returns whether it was used.
    pub fn text_input(&mut self, text: &str) -> bool {
        let Some(id) = self.focus else {
            return false;
        };
        let Kind::Control(Control::Edit(e)) = &mut self.nodes[id].kind else {
            return false;
        };
        if e.max_length == 0 {
            return false;
        }
        let mut changed = false;
        // `EditElement::SelectCharacter`: the keys of a US keyboard give printable ASCII but
        // `:` and `;`; each goes in while the text is shorter than its most and the edit
        // takes it. Deviation: an edit whose allowed characters name `:` or `;` takes them (the
        // Multiplayer window's relay address); the original's edits never name them.
        for ch in text.chars() {
            let typable =
                (' '..='~').contains(&ch) && (!matches!(ch, ':' | ';') || e.allowed.contains(ch));
            if !typable
                || e.text.text.chars().count() >= e.max_length
                || (!e.allowed.is_empty() && !e.allowed.contains(ch))
            {
                continue;
            }
            let at = byte_index(&e.text.text, e.cursor);
            e.text.text.insert(at, ch);
            e.cursor += 1;
            changed = true;
        }
        if changed {
            self.pending.push(Event::Changed(id));
        }
        true
    }

    /// `EditElement::Update` losing the focus (Enter, Escape or a press elsewhere): a
    /// NumberEdit's text becomes its number within its range (`NumberEditElement::
    /// FinalizeText`), and event `0x15` goes.
    pub(crate) fn commit_edit(&mut self, id: NodeId) {
        if let Kind::Control(Control::Edit(e)) = &mut self.nodes[id].kind {
            if let Some(range) = e.range {
                let value = to_integer(&e.text.text)
                    .clamp(range[0].min(range[1]) as i32, range[0].max(range[1]) as i32);
                e.text.text = value.to_string();
                e.cursor = e.cursor.min(e.text.text.chars().count());
            }
            self.pending.push(Event::Submitted(id));
        }
        if self.focus == Some(id) {
            self.focus = None;
        }
    }

    /// The caret's blink: every half second while an edit has the focus.
    pub(crate) fn update_caret(&mut self, dt: f32) {
        let Some(id) = self.focus else { return };
        if let Kind::Control(Control::Edit(e)) = &mut self.nodes[id].kind {
            e.blink += dt;
            if 0.5 < e.blink {
                e.caret = !e.caret;
                e.blink = 0.0;
            }
        }
    }
}
