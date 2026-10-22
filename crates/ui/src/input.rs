mod keyboard;
mod pointer;
mod tooltip;

use crate::controls::{intersect, scroll_thumb};
use crate::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// A key the interface reacts to.
pub enum Key {
    /// Tab: the next control.
    Tab,
    /// Shift and Tab: the previous control.
    BackTab,
    /// Enter.
    Enter,
    /// Escape.
    Escape,
    /// The space bar.
    Space,
    /// The left arrow.
    Left,
    /// The right arrow.
    Right,
    /// The up arrow.
    Up,
    /// The down arrow.
    Down,
    /// Home.
    Home,
    /// End.
    End,
    /// Page Up.
    PageUp,
    /// Page Down.
    PageDown,
    /// Backspace.
    Backspace,
    /// Delete.
    Delete,
    /// A character key.
    Character(char),
}

impl Ui {
    /// The element that has the keyboard focus.
    pub fn focused(&self) -> Option<NodeId> {
        self.focus
    }
    /// The pointer in UI units, as last moved.
    pub fn pointer(&self) -> (i32, i32) {
        self.pointer
    }
    /// Whether the current pointer target is a control that consumes pointer input.
    pub fn pointer_over_interactive(&self) -> bool {
        self.hot.is_some_and(|id| self.interactive(id))
    }
    /// `DialogInstance::GetMouseCursorIndex`: the cursor index of `id`, unless one of its
    /// visible children holds `(x, y)`, then that child's, the last such child winning.
    pub fn cursor_index(&self, id: NodeId, x: i32, y: i32) -> i32 {
        let n = &self.nodes[id];
        let mut cursor = n.cursor;
        for &c in &n.children {
            let child = &self.nodes[c];
            if child.visible() && child.contains(x, y) {
                cursor = self.cursor_index(c, x, y);
            }
        }
        cursor
    }

    /// Whether key input should be reserved for an active text edit control.
    pub fn text_edit_focused(&self) -> bool {
        self.focus
            .is_some_and(|id| matches!(&self.nodes[id].kind, Kind::Control(Control::Edit(_))))
    }
    /// The events since the last call, in order. (A click is returned by
    /// [`pointer_button`](Self::pointer_button) instead.)
    pub fn drain_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.pending)
    }
    /// Set the key that clicks an element: an upper-case character code, or 13 (Enter) or 27
    /// (Escape).
    pub fn set_shortcut(&mut self, id: NodeId, key: u32) {
        self.nodes[id].shortcut = Some(key);
    }
    /// Forget the pointer, the focus and the pending events, for example when the window loses the
    /// focus.
    pub fn clear_input(&mut self) {
        self.focus = None;
        self.hot = None;
        self.pressed = None;
        self.pointer_down = false;
        self.pending.clear();
        self.input_root = None;
        self.tooltip_instance = None;
    }
}

fn contains(r: [i32; 4], x: i32, y: i32) -> bool {
    x >= r[0] && x < r[2] && y >= r[1] && y < r[3]
}

pub(crate) fn event_node(e: Event) -> NodeId {
    match e {
        Event::Clicked(n)
        | Event::Selected(n, _)
        | Event::Changed(n)
        | Event::Submitted(n)
        | Event::Link(n, _) => n,
    }
}

/// The type ids (CRC-32 of the class name) of the game's tooltip controllers that the interface
/// handles itself.
pub mod tooltip_controller {
    /// `HotKeyToolTipUI`: a tooltip that shows the element's key.
    pub const HOT_KEY: u32 = 0x5dd8_3c93;
    /// `TitleDescToolTipUI`: a tooltip with a title and a description.
    pub const TITLE_DESC: u32 = 0x33e6_1003;
    /// The tooltip that lists what a building needs.
    pub const BUILD_REQUIREMENT: u32 = 0xc3fe_f549;
}
