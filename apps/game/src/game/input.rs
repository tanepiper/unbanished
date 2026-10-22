//! The player's input: the pointer, the wheel, keys, and the probe's presses and typing.

use super::*;

pub(crate) fn ui_key(key: &Key, shift: bool) -> Option<unbanished_ui::Key> {
    use unbanished_ui::Key as K;
    Some(match key {
        Key::Named(NamedKey::Tab) => {
            if shift {
                K::BackTab
            } else {
                K::Tab
            }
        }
        Key::Named(NamedKey::Enter) => K::Enter,
        Key::Named(NamedKey::Escape) => K::Escape,
        Key::Named(NamedKey::Space) => K::Space,
        Key::Named(NamedKey::ArrowDown) => K::Down,
        Key::Named(NamedKey::ArrowUp) => K::Up,
        Key::Named(NamedKey::ArrowLeft) => K::Left,
        Key::Named(NamedKey::ArrowRight) => K::Right,
        Key::Named(NamedKey::PageDown) => K::PageDown,
        Key::Named(NamedKey::PageUp) => K::PageUp,
        Key::Named(NamedKey::Home) => K::Home,
        Key::Named(NamedKey::End) => K::End,
        Key::Named(NamedKey::Backspace) => K::Backspace,
        Key::Named(NamedKey::Delete) => K::Delete,
        Key::Character(s) => K::Character(s.chars().next()?),
        _ => return None,
    })
}

impl Game {
    /// The pointer at window pixel `(x, y)`.
    pub fn pointer_move(&mut self, x: i32, y: i32) {
        self.pointer = Some((x, y));
        let s = self.ui_scale();
        self.pointer_move_ui((x as f32 / s) as i32, (y as f32 / s) as i32);
    }

    /// The pointer at UI position `(x, y)`.
    pub(crate) fn pointer_move_ui(&mut self, x: i32, y: i32) {
        if let Some(root) = self.input_root() {
            self.ui.pointer_move(x, y, root);
        }
        self.raw(RawInput::Cursor(x, y));
    }

    pub fn pointer_button(&mut self, down: bool) -> bool {
        self.audio.unlock();
        if let Some(root) = self.input_root() {
            let captured = self.ui.pointer_over_interactive();
            if let Some(event) = self.ui.pointer_button(down, root) {
                self.with_ctx(|main, ctx| main.inject(ctx, event));
            }
            self.deliver_events();
            if down && !captured {
                self.raw(RawInput::Click);
            } else if down {
                self.raw(RawInput::UiPress);
            } else if !down {
                self.raw(RawInput::Release);
            }
            captured
        } else {
            false
        }
    }

    pub fn wheel(&mut self, steps: f32) {
        if self.input_root().is_some() && self.ui.wheel((steps * 40.0) as i32) {
            self.deliver_events();
        } else {
            self.raw(RawInput::Wheel(steps));
        }
    }

    pub fn drag(&mut self, dx: f32, dy: f32) {
        self.raw(RawInput::Drag(dx, dy));
    }

    /// A key pressed: `key` for the UI (focus, edit boxes, shortcuts), `code` the engine's
    /// keyboard code for the key map.
    pub fn key(&mut self, key: Option<unbanished_ui::Key>, text: Option<&str>, code: Option<u8>) {
        self.audio.unlock();
        let handled = match (self.input_root(), key) {
            (Some(root), Some(key)) => {
                let handled = self.ui.key(root, key);
                if !handled {
                    if let Some(text) = text.filter(|t| t.chars().all(|c| !c.is_control())) {
                        self.ui.text_input(text);
                    }
                }
                self.deliver_events();
                handled || self.ui.text_edit_focused()
            }
            _ => false,
        };
        if handled {
            return;
        }
        let Some(code) = code else { return };
        let actions: Vec<usize> = self
            .settings
            .keymap
            .actions_for((keymap::KEYBOARD, code))
            .collect();
        for id in actions {
            use keymap::id;
            let raw = match id {
                id::CANCEL => RawInput::Escape,
                id => RawInput::Action(id),
            };
            self.raw(raw);
        }
    }

    /// A mouse button other than the left pressed: its key-map actions.
    pub fn pressed_mouse(&mut self, code: u8) {
        let actions: Vec<usize> = self
            .settings
            .keymap
            .actions_for((keymap::MOUSE, code))
            .collect();
        for id in actions {
            self.raw(RawInput::Action(id));
        }
    }

    /// A key or mouse button released (`Input::Device::Released`), for the key-binding box.
    pub fn released(&mut self, binding: keymap::Binding) {
        self.raw(RawInput::Released(binding));
        let actions: Vec<usize> = self.settings.keymap.actions_for(binding).collect();
        for id in actions {
            self.raw(RawInput::ActionReleased(id));
        }
    }

    /// Click the named edit, empty it and type `text` (scripted runs).
    pub fn type_into(&mut self, name: &str, text: &str, size: (u32, u32)) -> bool {
        if !self.press(name, size) {
            return false;
        }
        let Some(edit) = self.ui.focused() else {
            return false;
        };
        self.ui.set_text(edit, "");
        self.ui.text_input(text)
    }

    /// Click the named control through the normal pointer path (scripted runs): lay out the
    /// screen at `size`, move to the control's centre, press and release.
    pub fn press(&mut self, name: &str, size: (u32, u32)) -> bool {
        let size = self.logical(size);
        let Some(screen) = self.main.current().screen() else {
            return false;
        };
        screen.layout(&mut self.ui, size);
        let found = screen
            .dialogs
            .iter()
            .rev()
            .find_map(|d| self.ui.find(d.root, name));
        let Some(id) = found else { return false };
        self.press_node(id);
        true
    }

    /// Click an element through the normal pointer path: move to its centre (after the last
    /// layout), press and release.
    pub fn press_node(&mut self, id: unbanished_ui::NodeId) {
        let [x0, y0, x1, y1] = self.ui.node(id).rect;
        self.pointer_move_ui((x0 + x1) / 2, (y0 + y1) / 2);
        self.pointer_button(true);
        self.pointer_button(false);
    }

    pub fn select_combo(&mut self, name: &str, item: &str, size: (u32, u32)) -> bool {
        let size = self.logical(size);
        let Some(screen) = self.main.current().screen() else {
            return false;
        };
        screen.layout(&mut self.ui, size);
        let Some(combo) = screen
            .dialogs
            .iter()
            .rev()
            .find_map(|d| self.ui.find(d.root, name))
        else {
            return false;
        };
        let Some(list) = crate::menu::combo_list(&self.ui, combo) else {
            return false;
        };
        let index = match &self.ui.node(list).kind {
            unbanished_ui::Kind::List(data) => {
                data.items.iter().position(|t| t.eq_ignore_ascii_case(item))
            }
            _ => None,
        };
        let Some(index) = index else { return false };
        crate::menu::select_combo(&mut self.ui, combo, index);
        true
    }
}
