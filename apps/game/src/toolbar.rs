//! The in-game toolbar (`Dialog/Toolbar.rsc`), ported from `FUN_1400bede0` (build a level from the
//! registered `Toolbar` entries) and `FUN_1400be770` (handle clicks). See
//! docs/IN-GAME-TOOLBAR.md.
//!
//! Every `Game/Toolbar.rsc:*` entry names its parent; a level lists the visible entries with that
//! parent, sorted by `order`. Each entry becomes one of the dialog's templates, chosen by kind:
//! the speed readout (10), a sub-menu toggle (0), a build toggle (1) or a plain button (others).
//! Build and utility entries take their icon and name from the template's toolbar component.

use banished_formats::crc::type_id;
use banished_formats::game::{toolbar_description, toolbar_item, ToolbarDescription, ToolbarItem};
use banished_formats::pkg::Archive;
use banished_formats::registry::Registry;
use banished_ui::{Kind, NodeId, Ui};

use crate::keymap::KeyMap;
use crate::loops::Screen;

const DIALOG: &str = "Dialog/Toolbar.rsc:resource";
const TEMPLATES: [&str; 4] = [
    r"Dialog\Toolbar.rsc:textSpeed",
    r"Dialog\Toolbar.rsc:checkToolbar",
    r"Dialog\Toolbar.rsc:checkToolbar2",
    r"Dialog\Toolbar.rsc:buttonToolbar",
];

/// Item kinds (`Toolbar +0x48`), as dispatched by `FUN_1400be770`.
pub mod kind {
    pub const MENU: u32 = 0;
    pub const BUILD: u32 = 1;
    pub const UTILITY: u32 = 2;
    pub const OPTIONS: u32 = 3;
    pub const TOWN_INFO: u32 = 4;
    pub const CAMERA: u32 = 5;
    pub const SLOWER: u32 = 6;
    pub const PAUSE: u32 = 7;
    pub const PLAY: u32 = 8;
    pub const FASTER: u32 = 9;
    pub const SPEED: u32 = 10;
}

pub struct Entry {
    /// `Game\Toolbar.rsc:<name>`.
    pub reference: String,
    pub item: ToolbarItem,
    pub description: Option<ToolbarDescription>,
}

/// `FUN_14009f0f0`: the action's primary binding's display name (`keyNames`).
pub fn key_name(ui: &mut Ui, keymap: &KeyMap, action: i32) -> Option<String> {
    let binding = keymap.binding(usize::try_from(action).ok()?, 0);
    Some(ui.res.string(
        Some(r"Dialog\StringTable.rsc:keyNames"),
        &crate::keymap::key_name_key(binding),
    ))
}

/// `FUN_1400bede0`: the action ids a level hands out, by depth (root `Toolbar0..8`, then
/// `Tool0..9`, then `Camera0..4`).
fn key_pool(depth: usize) -> Vec<i32> {
    match depth {
        0 => (0x18..=0x20).collect(),
        1 => (0x23..=0x2c).collect(),
        _ => (0x2d..=0x31).collect(),
    }
}

/// What a click asks the game to do.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Slower,
    Faster,
    Pause,
    Play,
    /// Enter placement mode for a template (`None` leaves it).
    Build(Option<String>),
    /// Open a utility window (Status Bar, Event Log, …) for its template.
    Utility(String),
    Options,
    TownInfo,
    Camera(i32),
}

struct Level {
    /// Root element of this level's `Dialog/Toolbar.rsc` instance.
    root: NodeId,
    parent: Option<usize>,
    /// (button node, entry index).
    buttons: Vec<(NodeId, usize)>,
    /// Each button's key-map action (element `+0xc0`), parallel to `buttons`.
    actions: Vec<Option<i32>>,
    speed_text: Option<NodeId>,
}

pub struct Toolbar {
    entries: Vec<Entry>,
    root: Option<Level>,
    popup: Option<Level>,
    /// The bindings the hotkey tooltips name.
    keymap: KeyMap,
}

fn object_name(reference: &str) -> &str {
    reference.rsplit(':').next().unwrap_or(reference)
}

impl Toolbar {
    /// Load every registered toolbar entry (`ExternalFactory::FindNext` over the `Toolbar`
    /// type): the loaded resources' (the game's and the enabled mods', `formats::registry`).
    pub fn load(archive: &Archive, registry: &Registry) -> Toolbar {
        let mut entries = Vec::new();
        for reference in registry.of_type(type_id("Toolbar")) {
            let Ok(item) = archive.read_ref(reference).and_then(|d| toolbar_item(&d)) else {
                continue;
            };
            let description = item.template.as_deref().and_then(|t| {
                let r = format!("{}:toolbar", t.split(':').next()?.replace('\\', "/"));
                toolbar_description(&archive.read_ref(&r).ok()?).ok()
            });
            entries.push(Entry {
                reference: reference.to_string(),
                item,
                description,
            });
        }
        Toolbar {
            entries,
            root: None,
            popup: None,
            keymap: KeyMap::default(),
        }
    }

    /// A shown toolbar button by its item's name (`ToolbarDialog:<item>`).
    pub fn button(&self, ui: &Ui, name: &str) -> Option<NodeId> {
        [self.root.as_ref(), self.popup.as_ref()]
            .into_iter()
            .flatten()
            .find_map(|l| ui.find(l.root, name))
    }

    fn entry(&self, reference: &str) -> Option<&Entry> {
        let r = reference.replace('/', "\\");
        self.entries
            .iter()
            .find(|e| e.reference.eq_ignore_ascii_case(&r))
    }

    /// TutorialImageToolbarIcon (`FUN_1400e5e10`): the item's icon, or its template's
    /// (ToolbarDescription) when it places one.
    pub fn entry_sprite(&self, reference: &str) -> Option<(Option<String>, String)> {
        let e = self.entry(reference)?;
        match (&e.item.template, &e.description) {
            (Some(_), Some(d)) => Some((d.sprite_sheet.clone(), d.sprite.clone())),
            _ => Some((e.item.sprite_sheet.clone(), e.item.sprite.clone())),
        }
    }

    /// TutorialImageHotKey (`FUN_1400e58f0`): the item's key-map action (`+0x40`).
    pub fn entry_action(&self, reference: &str) -> Option<u32> {
        self.entry(reference).map(|e| e.item.value)
    }

    /// Entries of one level in display order.
    fn children(&self, parent: Option<usize>) -> Vec<usize> {
        let parent_ref = parent.map(|p| self.entries[p].reference.as_str());
        let mut list: Vec<usize> = (0..self.entries.len())
            .filter(|&i| {
                let e = &self.entries[i].item;
                e.visible
                    && match (e.parent.as_deref(), parent_ref) {
                        (None, None) => true,
                        (Some(p), Some(r)) => p.replace('/', "\\").eq_ignore_ascii_case(r),
                        _ => false,
                    }
            })
            .collect();
        list.sort_by_key(|&i| self.entries[i].item.order);
        list
    }

    fn build_level(
        &mut self,
        ui: &mut Ui,
        screen: &mut Screen,
        parent: Option<usize>,
    ) -> Option<Level> {
        let dialog = screen.add_dialog(ui, DIALOG)?;
        let content = screen.find(ui, dialog, "toolbarContent")?;
        let mut buttons = Vec::new();
        let mut actions = Vec::new();
        let mut speed_text = None;
        let mut depth = 0;
        let mut up = parent;
        while let Some(p) = up {
            depth += 1;
            up = self.entries[p].item.parent.as_deref().and_then(|r| {
                let r = r.replace('/', "\\");
                self.entries
                    .iter()
                    .position(|e| e.reference.eq_ignore_ascii_case(&r))
            });
        }
        let mut pool = key_pool(depth);
        for (slot, index) in self.children(parent).into_iter().enumerate() {
            let entry = &self.entries[index];
            let kind = entry.item.kind;
            // Element `+0xc0`: an entry that is not `enabled` keeps its own action id (and
            // takes it out of the pool); the others take the next free id.
            let action = if kind == kind::SPEED {
                None
            } else if !entry.item.enabled {
                let v = entry.item.value as i32;
                pool.retain(|&a| a != v);
                Some(v)
            } else if pool.is_empty() {
                None
            } else {
                Some(pool.remove(0))
            };
            let template = match kind {
                kind::SPEED => 0,
                kind::MENU => 1,
                kind::BUILD => 2,
                _ => 3,
            };
            let Some(node) = ui.instantiate(TEMPLATES[template]) else {
                continue;
            };
            ui.node_mut(node).name = object_name(&entry.reference).to_string();
            // Build and utility entries describe themselves through the template.
            let (table, tip, sheet, sprite) = match (&entry.description, kind) {
                (Some(d), kind::BUILD | kind::UTILITY) => (
                    d.string_table.as_deref(),
                    d.tooltip.as_str(),
                    d.sprite_sheet.as_deref(),
                    d.sprite.as_str(),
                ),
                _ => (
                    entry.item.string_table.as_deref(),
                    entry.item.tooltip.as_str(),
                    entry.item.sprite_sheet.as_deref(),
                    entry.item.sprite.as_str(),
                ),
            };
            if !tip.is_empty() {
                ui.node_mut(node).tooltip = ui.res.string(table, tip);
            }
            if let Some(a) = action {
                ui.node_mut(node).hotkey = key_name(ui, &self.keymap, a);
            }
            if !sprite.is_empty() {
                if let Some(s) = ui.res.sprite(sheet, sprite) {
                    ui.set_images(node, s);
                }
            }
            if kind == kind::SPEED {
                speed_text = Some(node);
            }
            ui.set_table_child(content, slot, node);
            buttons.push((node, index));
            actions.push(action);
        }
        Some(Level {
            root: screen.dialogs[dialog].root,
            parent,
            buttons,
            actions,
            speed_text,
        })
    }

    /// `FUN_14009d0d0`: add the top-level toolbar.
    pub fn install(&mut self, ui: &mut Ui, screen: &mut Screen, keymap: &KeyMap) {
        self.keymap = keymap.clone();
        self.root = self.build_level(ui, screen, None);
    }

    /// New bindings from the Options dialog: rename the shown buttons' hotkeys.
    pub fn set_keymap(&mut self, ui: &mut Ui, keymap: &KeyMap) {
        if self.keymap == *keymap {
            return;
        }
        self.keymap = keymap.clone();
        for level in self.root.iter().chain(self.popup.iter()) {
            for (&(node, _), action) in level.buttons.iter().zip(&level.actions) {
                if let Some(a) = *action {
                    ui.node_mut(node).hotkey = key_name(ui, &self.keymap, a);
                }
            }
        }
    }

    /// A key-map action pressed (`Toolbar0..8`, `Tool0..9`, `Camera0..4`): act as a click on
    /// the shown button holding it. Check-style buttons toggle first, as a click would.
    pub fn press_action(
        &mut self,
        ui: &mut Ui,
        screen: &mut Screen,
        action: i32,
    ) -> Option<Option<Action>> {
        let node = self
            .popup
            .iter()
            .chain(self.root.iter())
            .flat_map(|l| l.buttons.iter().zip(&l.actions))
            .find(|(_, a)| **a == Some(action))
            .map(|(&(n, _), _)| n)?;
        if !ui.node(node).enabled {
            return Some(None);
        }
        if matches!(
            ui.node(node).kind,
            Kind::Control(banished_ui::Control::Check { .. })
        ) {
            let on = ui.checked(node);
            ui.set_checked(node, !on);
        }
        Some(self.click(ui, screen, node))
    }

    fn text_node(ui: &Ui, root: NodeId) -> Option<NodeId> {
        let mut stack = vec![root];
        while let Some(n) = stack.pop() {
            if matches!(ui.node(n).kind, Kind::Text(_)) {
                return Some(n);
            }
            stack.extend(ui.node(n).children.iter().copied());
        }
        None
    }

    /// The build template behind a toolbar button (`Element::GetUserData` for
    /// BuildRequirementUI), with its ToolbarDescription.
    pub fn build_entry(&self, node: NodeId) -> Option<(&str, &ToolbarDescription)> {
        let level = self.root.iter().chain(self.popup.iter());
        let (_, index) = level
            .flat_map(|l| l.buttons.iter())
            .find(|(n, _)| *n == node)?;
        let e = &self.entries[*index];
        if e.item.kind != kind::BUILD {
            return None;
        }
        Some((e.item.template.as_deref()?, e.description.as_ref()?))
    }

    /// The root toolbar's layout width (`GetRoot()->+0x60`), which the event ribbon offsets by.
    pub fn width(&self, ui: &Ui) -> i32 {
        self.root.as_ref().map_or(0, |l| {
            let r = ui.node(l.root).rect;
            r[2] - r[0]
        })
    }

    /// `FUN_1400be210`: show the current speed and pause state.
    pub fn refresh(&self, ui: &mut Ui, speed: u32, paused: bool) {
        for level in self.root.iter().chain(self.popup.iter()) {
            if let Some(text) = level.speed_text.and_then(|n| Self::text_node(ui, n)) {
                ui.set_text(text, &format!("{speed}x"));
            }
            for &(node, index) in &level.buttons {
                let enabled = match self.entries[index].item.kind {
                    kind::PAUSE => !paused,
                    kind::PLAY => paused,
                    _ => true,
                };
                ui.node_mut(node).enabled = enabled;
            }
        }
    }

    fn close_popup(&mut self, ui: &mut Ui, screen: &mut Screen) {
        if let Some(level) = self.popup.take() {
            screen.remove_root(ui, level.root);
        }
        if let Some(root) = &self.root {
            for &(node, _) in &root.buttons {
                ui.set_checked(node, false);
            }
        }
    }

    /// Handle a click or toggle on one of the toolbar's buttons. Returns the game action, if any.
    pub fn click(&mut self, ui: &mut Ui, screen: &mut Screen, node: NodeId) -> Option<Action> {
        let level_of = |level: &Option<Level>| {
            level.as_ref().and_then(|l| {
                l.buttons
                    .iter()
                    .find(|(n, _)| ui.descendant(*n, node))
                    .map(|&(n, i)| (n, i))
            })
        };
        let (button, index) = level_of(&self.root).or_else(|| level_of(&self.popup))?;
        let item = self.entries[index].item.clone();
        match item.kind {
            kind::MENU => {
                // Toggle the sub-menu, centred over the button and above the bar.
                let open = self.popup.as_ref().and_then(|p| p.parent) == Some(index);
                self.close_popup(ui, screen);
                if !open {
                    ui.set_checked(button, true);
                    self.popup = self.build_level(ui, screen, Some(index));
                    self.place_popup(ui, button);
                }
                None
            }
            kind::BUILD => {
                let on = ui.checked(button);
                if let Some(popup) = &self.popup {
                    for &(n, _) in &popup.buttons {
                        if n != button {
                            ui.set_checked(n, false);
                        }
                    }
                }
                Some(Action::Build(on.then(|| item.template.clone()).flatten()))
            }
            kind::UTILITY => item.template.map(Action::Utility),
            kind::OPTIONS => Some(Action::Options),
            kind::TOWN_INFO => Some(Action::TownInfo),
            kind::CAMERA => Some(Action::Camera(item.id)),
            kind::SLOWER => Some(Action::Slower),
            kind::FASTER => Some(Action::Faster),
            kind::PAUSE => Some(Action::Pause),
            kind::PLAY => Some(Action::Play),
            _ => None,
        }
    }

    /// Uncheck the open sub-menu's build buttons (the tool ended outside the toolbar).
    pub fn clear_build(&self, ui: &mut Ui) {
        if let Some(popup) = &self.popup {
            for &(node, index) in &popup.buttons {
                if self.entries[index].item.kind == kind::BUILD {
                    ui.set_checked(node, false);
                }
            }
        }
    }

    /// Position the sub-menu the way `FUN_1400be770` pads its root: right padding so it is centred
    /// on the button, bottom padding equal to the main bar's height.
    fn place_popup(&self, ui: &mut Ui, button: NodeId) {
        let (Some(popup), Some(root)) = (&self.popup, &self.root) else {
            return;
        };
        let (popup_root, bar_root) = (popup.root, root.root);
        let screen_w = ui.screen[0];
        let screen_h = ui.screen[1];
        let b = ui.node(button).rect;
        let popup_w = ui.node(popup_root).size[0];
        let bar_h = screen_h - ui.node(bar_root).rect[1];
        let right = (screen_w - (b[0] + b[2]) / 2 - popup_w / 2).max(0);
        ui.node_mut(popup_root).margins = [0, right, 0, bar_h];
    }

    /// Re-centre an open sub-menu after layout (its width is known only then).
    pub fn relayout(&self, ui: &mut Ui) {
        let Some(popup) = &self.popup else { return };
        let Some(parent) = popup.parent else { return };
        let button = self
            .root
            .as_ref()
            .and_then(|r| r.buttons.iter().find(|&&(_, i)| i == parent))
            .map(|&(n, _)| n);
        if let Some(button) = button {
            self.place_popup(ui, button);
        }
    }
}
