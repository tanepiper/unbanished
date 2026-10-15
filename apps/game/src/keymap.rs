//! `Input::KeyMap` (Runtime): the game's actions (`Game::RegisterInputs`, `FUN_14009e7c0`),
//! their two bindings each, and their persistence (`KeyMap::Load` `0x1800a80e0`, `KeyMap::Save`
//! `0x1800a8470`: `Input<Action>` and `Input<Action>Alt`, the value `device << 8 | input` or −1).
//!
//! Devices: 0 gamepad, 1 keyboard, 2 mouse. Keyboard input codes follow the engine's table (the
//! `Dialog\StringTable.rsc:keyNames` entries `Keyboard0..103`): Escape 0, F1–F12 1–12, digits
//! 13–22, A–Z 23–48, then punctuation, modifiers, navigation and the numeric keypad.

use crate::settings::Persistence;

#[allow(dead_code)]
pub const GAMEPAD: u8 = 0;
pub const KEYBOARD: u8 = 1;
pub const MOUSE: u8 = 2;

/// One binding: `(device, input)`.
pub type Binding = (u8, u8);

#[derive(Debug, Clone, PartialEq)]
pub struct Action {
    pub name: &'static str,
    /// The player may rebind it (mouse axes may not).
    pub mappable: bool,
    /// Primary and alternative binding.
    pub slots: [Option<Binding>; 2],
}

/// The game's action ids (`FUN_14009e7c0`); the full table, including ids the port's input
/// does not read yet (mouse buttons, PathMethod, screenshots).
#[allow(dead_code)]
pub mod id {
    pub const MOUSE_LEFT: usize = 5;
    pub const MOUSE_RIGHT: usize = 6;
    pub const MOUSE_MIDDLE: usize = 7;
    pub const CANCEL: usize = 8;
    pub const ROTATE_LEFT: usize = 9;
    pub const ROTATE_RIGHT: usize = 10;
    pub const FORWARD: usize = 11;
    pub const BACK: usize = 12;
    pub const LEFT: usize = 13;
    pub const RIGHT: usize = 14;
    pub const ZOOM_IN: usize = 15;
    pub const ZOOM_OUT: usize = 16;
    pub const PITCH_UP: usize = 17;
    pub const PITCH_DOWN: usize = 18;
    pub const STRUCTURE_LEFT: usize = 19;
    pub const STRUCTURE_RIGHT: usize = 20;
    pub const STRUCTURE_TYPE: usize = 21;
    pub const PATH_METHOD: usize = 22;
    pub const TOGGLE_PAUSE: usize = 23;
    pub const TOOLBAR0: usize = 24;
    pub const SCREENSHOT_NO_UI: usize = 33;
    pub const SCREENSHOT: usize = 34;
    pub const TOOL0: usize = 35;
    pub const CAMERA0: usize = 45;
    pub const CAMERA4: usize = 49;
    pub const COUNT: usize = 50;
}

/// `FUN_14009e7c0`'s registrations: id order, name, default binding, mappable.
const REGISTERED: [(&str, u8, u8, bool); id::COUNT] = [
    ("MouseX", MOUSE, 0, false),
    ("MouseY", MOUSE, 1, false),
    ("MouseDeltaX", MOUSE, 2, false),
    ("MouseDeltaY", MOUSE, 3, false),
    ("MouseWheel", MOUSE, 0xc, false),
    ("MouseLeft", MOUSE, 4, true),
    ("MouseRight", MOUSE, 5, true),
    ("MouseMiddle", MOUSE, 6, true),
    ("Cancel", KEYBOARD, 0, true),
    ("RotateLeft", KEYBOARD, 0x27, true),
    ("RotateRight", KEYBOARD, 0x1b, true),
    ("Forward", KEYBOARD, 0x2d, true),
    ("Back", KEYBOARD, 0x29, true),
    ("Left", KEYBOARD, 0x17, true),
    ("Right", KEYBOARD, 0x1a, true),
    ("ZoomIn", KEYBOARD, 0x4d, true),
    ("ZoomOut", KEYBOARD, 0x50, true),
    ("PitchUp", KEYBOARD, 0x4f, true),
    ("PitchDown", KEYBOARD, 0x52, true),
    ("StructureLeft", KEYBOARD, 0x28, true),
    ("StructureRight", KEYBOARD, 0x2a, true),
    ("StructureType", KEYBOARD, 0x1c, true),
    ("PathMethod", KEYBOARD, 0x3e, true),
    ("TogglePause", KEYBOARD, 0x47, true),
    ("Toolbar0", KEYBOARD, 1, true),
    ("Toolbar1", KEYBOARD, 2, true),
    ("Toolbar2", KEYBOARD, 3, true),
    ("Toolbar3", KEYBOARD, 4, true),
    ("Toolbar4", KEYBOARD, 5, true),
    ("Toolbar5", KEYBOARD, 6, true),
    ("Toolbar6", KEYBOARD, 7, true),
    ("Toolbar7", KEYBOARD, 8, true),
    ("Toolbar8", KEYBOARD, 9, true),
    ("ScreenshotNoUI", KEYBOARD, 0x4a, true),
    ("Screenshot", KEYBOARD, 0x4b, true),
    ("Tool0", KEYBOARD, 0xe, true),
    ("Tool1", KEYBOARD, 0xf, true),
    ("Tool2", KEYBOARD, 0x10, true),
    ("Tool3", KEYBOARD, 0x11, true),
    ("Tool4", KEYBOARD, 0x12, true),
    ("Tool5", KEYBOARD, 0x13, true),
    ("Tool6", KEYBOARD, 0x14, true),
    ("Tool7", KEYBOARD, 0x15, true),
    ("Tool8", KEYBOARD, 0x16, true),
    ("Tool9", KEYBOARD, 0xd, true),
    ("Camera0", KEYBOARD, 0x30, true),
    ("Camera1", KEYBOARD, 0x2e, true),
    ("Camera2", KEYBOARD, 0x19, true),
    ("Camera3", KEYBOARD, 0x2c, true),
    ("Camera4", KEYBOARD, 0x18, true),
];

#[derive(Debug, Clone, PartialEq)]
pub struct KeyMap {
    pub actions: Vec<Action>,
}

impl Default for KeyMap {
    /// `KeyMap::ClearAll` then every `RegisterInput`.
    fn default() -> Self {
        KeyMap {
            actions: REGISTERED
                .iter()
                .map(|&(name, device, input, mappable)| Action {
                    name,
                    mappable,
                    slots: [Some((device, input)), None],
                })
                .collect(),
        }
    }
}

fn suffix(slot: usize) -> &'static str {
    if slot == 0 {
        ""
    } else {
        "Alt"
    }
}

impl KeyMap {
    /// `FUN_14009e7c0` + `KeyMap::Load`: the defaults, then each mappable action's stored
    /// bindings (−1 unbound).
    pub fn load(p: &Persistence) -> KeyMap {
        let mut map = KeyMap::default();
        for a in map.actions.iter_mut().filter(|a| a.mappable) {
            for slot in 0..2 {
                let mut v = match a.slots[slot] {
                    Some((d, i)) => (d as i32) << 8 | i as i32,
                    None => -1,
                };
                p.get_int("Input", &format!("{}{}", a.name, suffix(slot)), &mut v);
                a.slots[slot] = (v >= 0).then_some(((v >> 8) as u8, (v & 0xff) as u8));
            }
        }
        map
    }

    /// `KeyMap::Save`.
    pub fn save(&self, p: &mut Persistence) {
        for a in self.actions.iter().filter(|a| a.mappable) {
            for slot in 0..2 {
                let v = a.slots[slot].map_or(-1, |(d, i)| (d as i32) << 8 | i as i32);
                p.set_int("Input", &format!("{}{}", a.name, suffix(slot)), v);
            }
        }
    }

    /// `KeyMap::GetIdForDeviceInput`: the first action bound to this input.
    pub fn id_for(&self, binding: Binding) -> Option<usize> {
        self.actions
            .iter()
            .position(|a| a.slots.contains(&Some(binding)))
    }

    /// `KeyMap::SetInput`.
    pub fn set(&mut self, id: usize, slot: usize, binding: Option<Binding>) {
        if let Some(a) = self.actions.get_mut(id) {
            a.slots[slot.min(1)] = binding;
        }
    }

    /// `KeyMap::ClearInput(id, device, input)`: drop that binding from the action.
    pub fn clear_input(&mut self, id: usize, binding: Binding) {
        if let Some(a) = self.actions.get_mut(id) {
            for s in &mut a.slots {
                if *s == Some(binding) {
                    *s = None;
                }
            }
        }
    }

    pub fn binding(&self, id: usize, slot: usize) -> Option<Binding> {
        self.actions.get(id)?.slots[slot.min(1)]
    }

    /// Whether `binding` triggers action `id`.
    pub fn is(&self, id: usize, binding: Binding) -> bool {
        self.actions
            .get(id)
            .is_some_and(|a| a.slots.contains(&Some(binding)))
    }

    /// The actions a pressed input triggers.
    pub fn actions_for(&self, binding: Binding) -> impl Iterator<Item = usize> + '_ {
        self.actions
            .iter()
            .enumerate()
            .filter(move |(_, a)| a.slots.contains(&Some(binding)))
            .map(|(i, _)| i)
    }
}

/// `GameGlobal::KeyName` (`FUN_14009f0f0`): the `keyNames` key of a binding.
pub fn key_name_key(binding: Option<Binding>) -> String {
    match binding {
        None => "Unbound".into(),
        Some((d, i)) => {
            let device = ["Gamepad", "Keyboard", "Mouse"]
                .get(d as usize)
                .copied()
                .unwrap_or("Keyboard");
            format!("{device}{i}")
        }
    }
}

/// The engine's keyboard code for a physical key (the order of `keyNames:Keyboard0..103`).
pub fn keyboard_code(code: &winit::keyboard::KeyCode) -> Option<u8> {
    use winit::keyboard::KeyCode as K;
    Some(match code {
        K::Escape => 0,
        K::F1 => 1,
        K::F2 => 2,
        K::F3 => 3,
        K::F4 => 4,
        K::F5 => 5,
        K::F6 => 6,
        K::F7 => 7,
        K::F8 => 8,
        K::F9 => 9,
        K::F10 => 10,
        K::F11 => 11,
        K::F12 => 12,
        K::Digit0 => 13,
        K::Digit1 => 14,
        K::Digit2 => 15,
        K::Digit3 => 16,
        K::Digit4 => 17,
        K::Digit5 => 18,
        K::Digit6 => 19,
        K::Digit7 => 20,
        K::Digit8 => 21,
        K::Digit9 => 22,
        K::KeyA => 23,
        K::KeyB => 24,
        K::KeyC => 25,
        K::KeyD => 26,
        K::KeyE => 27,
        K::KeyF => 28,
        K::KeyG => 29,
        K::KeyH => 30,
        K::KeyI => 31,
        K::KeyJ => 32,
        K::KeyK => 33,
        K::KeyL => 34,
        K::KeyM => 35,
        K::KeyN => 36,
        K::KeyO => 37,
        K::KeyP => 38,
        K::KeyQ => 39,
        K::KeyR => 40,
        K::KeyS => 41,
        K::KeyT => 42,
        K::KeyU => 43,
        K::KeyV => 44,
        K::KeyW => 45,
        K::KeyX => 46,
        K::KeyY => 47,
        K::KeyZ => 48,
        K::Backquote => 49,
        K::Minus => 50,
        K::Equal => 51,
        K::Backspace => 52,
        K::Tab => 53,
        K::BracketLeft => 54,
        K::BracketRight => 55,
        K::Backslash => 56,
        K::CapsLock => 57,
        K::Semicolon => 58,
        K::Quote => 59,
        K::Enter => 60,
        K::ShiftLeft => 62,
        K::Comma => 63,
        K::Period => 64,
        K::Slash => 65,
        K::ShiftRight => 66,
        K::ControlLeft => 69,
        K::AltLeft => 70,
        K::Space => 71,
        K::AltRight => 72,
        K::ControlRight => 73,
        K::PrintScreen => 74,
        K::ScrollLock => 75,
        K::Pause => 76,
        K::Insert => 77,
        K::Home => 78,
        K::PageUp => 79,
        K::Delete => 80,
        K::End => 81,
        K::PageDown => 82,
        K::ArrowUp => 83,
        K::ArrowLeft => 84,
        K::ArrowDown => 85,
        K::ArrowRight => 86,
        K::NumLock => 87,
        K::NumpadDivide => 88,
        K::NumpadMultiply => 89,
        K::NumpadSubtract => 90,
        K::NumpadDecimal => 91,
        K::NumpadAdd => 92,
        K::NumpadEnter => 93,
        K::Numpad0 => 94,
        K::Numpad1 => 95,
        K::Numpad2 => 96,
        K::Numpad3 => 97,
        K::Numpad4 => 98,
        K::Numpad5 => 99,
        K::Numpad6 => 100,
        K::Numpad7 => 101,
        K::Numpad8 => 102,
        K::Numpad9 => 103,
        _ => return None,
    })
}

/// The engine's mouse input code for a button (`keyNames:Mouse4..11`).
pub fn mouse_code(button: winit::event::MouseButton) -> Option<u8> {
    use winit::event::MouseButton as B;
    Some(match button {
        B::Left => 4,
        B::Right => 5,
        B::Middle => 6,
        B::Back => 7,
        B::Forward => 8,
        B::Other(n) if n < 3 => 9 + n as u8,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_registrations_and_persist() {
        let mut map = KeyMap::default();
        assert_eq!(map.binding(id::FORWARD, 0), Some((KEYBOARD, 45)));
        assert_eq!(map.id_for((KEYBOARD, 71)), Some(id::TOGGLE_PAUSE));
        assert_eq!(key_name_key(map.binding(id::TOOL0, 0)), "Keyboard14");
        map.set(id::FORWARD, 1, Some((KEYBOARD, 83)));
        map.clear_input(id::BACK, (KEYBOARD, 41));
        let mut p = Persistence::default();
        map.save(&mut p);
        let text = p.serialize();
        assert!(text.contains("InputForwardAlt=339"));
        assert!(text.contains("InputBack=-1"));
        assert!(!text.contains("InputMouseX"));
        let mut q = Persistence::default();
        q.parse(&text);
        assert_eq!(KeyMap::load(&q), map);
    }
}
