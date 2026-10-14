//! The in-game toolbar (`Dialog/Toolbar.rsc`), ported from `FUN_1400bede0` (build a level from the
//! registered `Toolbar` entries) and `FUN_1400be770` (handle clicks). See
//! docs/IN-GAME-TOOLBAR.md.
//!
//! Every `Game/Toolbar.rsc:*` entry names its parent; a level lists the visible entries with that
//! parent, sorted by `order`. Each entry becomes one of the dialog's templates, chosen by kind:
//! the speed readout (10), a sub-menu toggle (0), a build toggle (1) or a plain button (others).
//! Build and utility entries take their icon and name from the template's toolbar component.

use banished_formats::game::{toolbar_description, toolbar_item, ToolbarDescription, ToolbarItem};
use banished_formats::pkg::Archive;
use banished_ui::{Kind, NodeId, Ui};

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
    speed_text: Option<NodeId>,
}

pub struct Toolbar {
    entries: Vec<Entry>,
    root: Option<Level>,
    popup: Option<Level>,
}

fn object_name(reference: &str) -> &str {
    reference.rsplit(':').next().unwrap_or(reference)
}

impl Toolbar {
    /// Load every registered toolbar entry (`ExternalFactory::FindNext` over the `Toolbar` type).
    pub fn load(archive: &Archive) -> Toolbar {
        let mut entries = Vec::new();
        for name in archive.names() {
            let path = name.replace('\\', "/");
            let Some(object) = path
                .strip_prefix("Game/Toolbar_")
                .and_then(|n| n.strip_suffix(".crs"))
            else {
                continue;
            };
            if object == "resource" {
                continue;
            }
            let Ok(item) = archive.read(name).and_then(|d| toolbar_item(&d)) else {
                continue;
            };
            let description = item.template.as_deref().and_then(|t| {
                let r = format!("{}:toolbar", t.split(':').next()?.replace('\\', "/"));
                toolbar_description(&archive.read_ref(&r).ok()?).ok()
            });
            entries.push(Entry {
                reference: format!(r"Game\Toolbar.rsc:{object}"),
                item,
                description,
            });
        }
        Toolbar {
            entries,
            root: None,
            popup: None,
        }
    }

    /// Entries of one level in display order.
    fn children(&self, parent: Option<usize>) -> Vec<usize> {
        let parent_ref = parent.map(|p| self.entries[p].reference.as_str());
        let mut list: Vec<usize> = (0..self.entries.len())
            .filter(|&i| {
                let e = &self.entries[i].item;
                e.visible
                    && e.parent.as_deref().map(|p| p.replace('/', "\\")).as_deref() == parent_ref
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
        let mut speed_text = None;
        for (slot, index) in self.children(parent).into_iter().enumerate() {
            let entry = &self.entries[index];
            let kind = entry.item.kind;
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
        }
        Some(Level {
            root: screen.dialogs[dialog].root,
            parent,
            buttons,
            speed_text,
        })
    }

    /// `FUN_14009d0d0`: add the top-level toolbar.
    pub fn install(&mut self, ui: &mut Ui, screen: &mut Screen) {
        self.root = self.build_level(ui, screen, None);
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
