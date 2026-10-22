//! Utility windows opened from the toolbar (Status Bar, Event Log, Professions, …), ported from
//! `FUN_140106f50` and the Runtime `UI::DialogInstance` constructor. See docs/UTILITY-WINDOWS.md.
//!
//! A utility template's `:ui` component is a `UIDescription`: a host dialog
//! (`Dialog/StandardDialog.rsc`) plus a table of `{controller type, element, slot name}`. The
//! element is instantiated into the named slot of the host and the controller is attached to it.
//! Clicking the toolbar button again closes the window; the window's upper-left corner is
//! remembered per template (default 0, 0).

mod event_log;
mod help;
mod map;
mod professions;
mod status_bar;
pub use event_log::{format_event, EventLogUi};
pub use help::HelpUi;
pub use map::MapUi;
use professions::ProfessionUi;
pub use status_bar::StatusValues;
pub(crate) use status_bar::{set_parameters, text_of, StatusBar};

use std::collections::BTreeMap;

use unbanished_formats::game::{external_list, profession, ui_description, UiDescription};
use unbanished_formats::pkg::Archive;
use unbanished_ui::{Kind, NodeId, Ui};

use crate::loops::Screen;

/// CRC32 type ids of the `UI::DialogController` subclasses.
pub mod controller {
    pub const STATUS_BAR: u32 = 0x509d_1aea;
    pub const PROFESSION: u32 = 0xc841_e4d4;
    pub const EVENT_LOG: u32 = 0xb075_92fd;
    pub const MAP: u32 = 0x42c5_425e;
    pub const HELP: u32 = 0x2b7a_7bbc;
    /// `TutorialUI` (`0x33161e00`): bound by the tutorial (`gameplay/tutorial.rs`).
    pub const TUTORIAL: u32 = 0x3316_1e00;
}

/// What a click in a utility window asks of the game.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UtilityClick {
    /// Handled inside the window.
    Handled,
    /// `FUN_140067c80`: set notify flag `i`.
    Notify(usize, bool),
    /// `FUN_140067cd0`: move the camera to the cell.
    GoTo(i32),
    /// ProfessionUI spinner `300 + i` (`FUN_14007c0e0` → `FUN_14007b890`): the player set
    /// profession `i`'s count.
    ProfessionCount(usize, i32),
    /// ProfessionUI goto `500 + i` (`FUN_14007bf00`): the citizen to show.
    ShowCitizen(usize),
    /// ResourceLimitUI (`FUN_14007ccb0`): the stock slot's limit.
    StockLimit(usize, i32),
}

const STOCK_NAMES: [&str; 11] = [
    "numberWood",
    "numberStone",
    "numberIron",
    "numberFireWood",
    "numberTools",
    "numberFood",
    "numberHerbs",
    "numberClothes",
    "numberAlcohol",
    "numberCoal",
    "numberTextile",
];
const STATUS_NAMES: [&str; 6] = ["Home", "Food", "Cold", "Job", "Tool", "Disease"];
const WEATHER_NAMES: [&str; 5] = [
    "imageRain",
    "imageCloudy",
    "imagePartlySunny",
    "imageSunny",
    "imageSnow",
];

struct Window {
    template: String,
    root: NodeId,
    /// The UIDescription's controller type ids.
    ids: Vec<u32>,
    status_bar: Option<StatusBar>,
    professions: Option<ProfessionUi>,
    event_log: Option<EventLogUi>,
    map: Option<MapUi>,
    help: Option<HelpUi>,
    /// `ResourceLimitUI` rows (the Limits window).
    limits: Vec<crate::ui::building_ui::ResourceLimitUi>,
}

#[derive(Default)]
pub struct UtilityWindows {
    open: Vec<Window>,
    /// Upper-left corner per template (`param_1 + 0x50` map in `FUN_140106f50`).
    positions: BTreeMap<String, (i32, i32)>,
}

fn description(archive: &Archive, template: &str) -> Option<UiDescription> {
    let file = template.split(':').next()?.replace('\\', "/");
    let data = archive.read_ref(&format!("{file}:ui")).ok()?;
    ui_description(&data)
        .map_err(|e| log::warn!("{template}: {e}"))
        .ok()
}

impl UtilityWindows {
    /// `FUN_140106f50`: open the template's window, or close it if it is open.
    pub fn toggle(
        &mut self,
        ui: &mut Ui,
        screen: &mut Screen,
        archive: &Archive,
        template: &str,
        log: Option<&unbanished_game::events::EventLog>,
        map_size: u32,
    ) {
        if let Some(i) = self.open.iter().position(|w| w.template == template) {
            self.close(ui, screen, i);
            return;
        }
        let Some(desc) = description(archive, template) else {
            log::warn!("{template}: no UIDescription");
            return;
        };
        let Some(host) = desc.dialog.as_deref() else {
            return;
        };
        let Some(dialog) = screen.add_dialog(ui, &host.replace('\\', "/")) else {
            return;
        };
        let root = screen.dialogs[dialog].root;
        let mut status_bar = None;
        let mut professions = None;
        let mut event_log = None;
        let mut map = None;
        let mut help = None;
        let mut limits = Vec::new();
        for c in &desc.controllers {
            // `FUN_1401335d0`: instantiate the element into the named slot; a TabElement slot
            // would get a tab instead (no utility window uses one).
            let Some(slot) = ui.find(root, &c.slot) else {
                log::warn!("{template}: slot {} not found", c.slot);
                continue;
            };
            let Some(element) = c.element.as_deref().and_then(|e| ui.instantiate(e)) else {
                continue;
            };
            ui.add_child(slot, element);
            match c.type_id {
                0 | controller::TUTORIAL => {}
                controller::STATUS_BAR => status_bar = Some(StatusBar::bind(ui, root)),
                controller::PROFESSION => professions = Some(ProfessionUi::bind(ui, root, archive)),
                controller::EVENT_LOG => event_log = Some(EventLogUi::bind(ui, root, log)),
                controller::MAP => map = Some(MapUi::bind(ui, root, map_size)),
                controller::HELP => help = Some(HelpUi::bind(ui, root, archive)),
                crate::ui::building_ui::RESOURCE_LIMIT_UI => {
                    limits.push(crate::ui::building_ui::ResourceLimitUi::bind(
                        ui,
                        archive,
                        element,
                        c.config.as_deref(),
                    ));
                }
                other => log::info!("{template}: controller {other:#010x} not implemented yet"),
            }
        }
        // Inferred: the host's `labelTitle` shows the utility's toolbar name ("Professions").
        if let Some(title) = ui.find(root, "labelTitle") {
            let file = template.split(':').next().unwrap_or("").replace('\\', "/");
            if let Some(t) = archive
                .read_ref(&format!("{file}:toolbar"))
                .ok()
                .and_then(|d| unbanished_formats::game::toolbar_description(&d).ok())
            {
                let text = ui.res.string(t.string_table.as_deref(), &t.name);
                ui.set_text(title, &text);
            }
        }
        let (x, y) = *self.positions.entry(template.to_string()).or_insert((0, 0));
        // `DialogInstance::SetUpperLeft`.
        let node = ui.node_mut(root);
        node.alignment = 0;
        node.margins = [x.max(0), 0, y.max(0), 0];
        self.open.push(Window {
            template: template.to_string(),
            root,
            status_bar,
            professions,
            event_log,
            map,
            help,
            limits,
            ids: desc.controllers.iter().map(|c| c.type_id).collect(),
        });
    }

    /// [`Self::toggle`] with the window's upper-left corner (`FUN_1401056c0`); its root.
    pub fn open_at(
        &mut self,
        ui: &mut Ui,
        screen: &mut Screen,
        archive: &Archive,
        template: &str,
        at: (i32, i32),
        map_size: u32,
    ) -> Option<NodeId> {
        if let Some(w) = self.open.iter().find(|w| w.template == template) {
            return Some(w.root);
        }
        self.positions.insert(template.to_string(), at);
        self.toggle(ui, screen, archive, template, None, map_size);
        self.open
            .iter()
            .find(|w| w.template == template)
            .map(|w| w.root)
    }

    /// The open window under `(x, y)` by its `borderTitle` (topmost first): its template and
    /// root.
    pub fn title_at(&self, ui: &Ui, x: i32, y: i32) -> Option<(String, NodeId)> {
        self.open.iter().rev().find_map(|w| {
            let b = ui.find(w.root, "borderTitle")?;
            let [x0, y0, x1, y1] = ui.node(b).rect;
            (x0 <= x && x < x1 && y0 <= y && y < y1).then(|| (w.template.clone(), w.root))
        })
    }

    /// Remember a window's upper-left corner (ToolUI `+0x50` map).
    pub fn remember(&mut self, template: &str, at: (i32, i32)) {
        self.positions.insert(template.to_string(), at);
    }

    /// The open windows' roots and controller type ids.
    pub fn controllers(&self) -> Vec<(NodeId, Vec<u32>)> {
        self.open.iter().map(|w| (w.root, w.ids.clone())).collect()
    }

    /// Close the windows with a controller of the type.
    pub fn close_with(&mut self, ui: &mut Ui, screen: &mut Screen, id: u32) {
        while let Some(i) = self.open.iter().position(|w| w.ids.contains(&id)) {
            self.close(ui, screen, i);
        }
    }

    /// Close the window whose root this is.
    pub fn close_root(&mut self, ui: &mut Ui, screen: &mut Screen, root: NodeId) {
        if let Some(i) = self.open.iter().position(|w| w.root == root) {
            self.close(ui, screen, i);
        }
    }

    fn close(&mut self, ui: &mut Ui, screen: &mut Screen, index: usize) {
        let w = self.open.remove(index);
        screen.remove_root(ui, w.root);
    }

    /// Close the window whose host dialog contains `node` (the title bar's close button).
    pub fn close_containing(&mut self, ui: &mut Ui, screen: &mut Screen, node: NodeId) -> bool {
        let Some(i) = self.open.iter().position(|w| ui.descendant(w.root, node)) else {
            return false;
        };
        self.close(ui, screen, i);
        true
    }

    /// A click inside a utility window's controller (EventLogUI, ProfessionUI).
    pub fn click(
        &mut self,
        ui: &Ui,
        node: NodeId,
        log: &unbanished_game::events::EventLog,
        values: &StatusValues,
    ) -> Option<UtilityClick> {
        for s in self.open.iter_mut().filter_map(|w| w.status_bar.as_mut()) {
            if let Some(c) = s.icon_click(ui, node, values) {
                return Some(UtilityClick::ShowCitizen(c));
            }
        }
        for p in self.open.iter_mut().filter_map(|w| w.professions.as_mut()) {
            if let Some(i) = p.goto(ui, node) {
                return p
                    .next_worker(i, &values.citizens)
                    .map(UtilityClick::ShowCitizen)
                    .or(Some(UtilityClick::Handled));
            }
        }
        if let Some((i, v)) = self
            .open
            .iter()
            .filter_map(|w| w.professions.as_ref())
            .find_map(|p| p.changed(ui, node, values))
        {
            return Some(UtilityClick::ProfessionCount(i, v));
        }
        if let Some(crate::ui::building_ui::WorkPlaceClick::StockLimit { slot, limit }) = self
            .open
            .iter()
            .flat_map(|w| &w.limits)
            .find_map(|l| l.click(ui, node))
        {
            return Some(UtilityClick::StockLimit(slot, limit));
        }
        self.open
            .iter_mut()
            .filter_map(|w| w.event_log.as_mut())
            .find_map(|e| e.click(ui, node, log))
    }

    /// An event for an open Help window (its combo, links and buttons).
    pub fn help_event(&mut self, ui: &mut Ui, event: unbanished_ui::Event) -> bool {
        self.open
            .iter_mut()
            .filter_map(|w| w.help.as_mut())
            .any(|h| h.event(ui, event))
    }

    /// A click on an open Map window's overlay: the camera target.
    pub fn map_click(&self, ui: &Ui, at: (i32, i32), size: u32) -> Option<glam::Vec2> {
        self.open
            .iter()
            .filter_map(|w| w.map.as_ref())
            .find_map(|m| m.click(ui, at, size))
    }

    /// `DialogInstance::OnUpdateLayout` for every open window's controllers.
    pub fn update(&self, ui: &mut Ui, values: &StatusValues) {
        for w in &self.open {
            if let Some(m) = &w.map {
                m.update(ui, values);
            }
            if let (Some(e), Some(log)) = (&w.event_log, &values.events) {
                e.update(ui, log);
            }
            if let Some(s) = &w.status_bar {
                s.update(ui, values);
            }
            if let Some(p) = &w.professions {
                p.update(ui, values);
            }
            if let Some(h) = &w.help {
                h.update(ui);
            }
            for l in &w.limits {
                l.refresh_limits(ui, &values.stock_limits);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The goto button cycles through the profession's working adults from after the last one
    /// shown; with none of them, the citizen reached after as many misses as working adults.
    #[test]
    fn goto_cycles_through_a_professions_workers() {
        let mut p = ProfessionUi {
            spinners: Vec::new(),
            available: Vec::new(),
            gotos: Vec::new(),
            cursor: 0,
        };
        // (citizen, profession, working adult): 10 and 13 are farmers (2), 12 a child farmer.
        let town = [
            (10, 2, true, 0),
            (11, 1, true, 0),
            (12, 2, false, 0),
            (13, 2, true, 0),
        ];
        assert_eq!(p.next_worker(2, &town), Some(13));
        assert_eq!(p.next_worker(2, &town), Some(10));
        assert_eq!(p.next_worker(2, &town), Some(13));
        // No hunters: after three misses (three working adults), whoever is reached.
        assert_eq!(p.next_worker(5, &town), Some(13));
        assert_eq!(p.next_worker(5, &[(1, 0, false, 0)]), None);
    }

    /// StatusBarUI's icons (`FUN_14007f5f0`): a click on the home icon (bit 0) goes to the
    /// next homeless citizen after the last one, round again; none in a tutorial.
    #[test]
    fn status_icons_cycle_through_their_citizens() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../game-data/original/WinData");
        let mut archive = unbanished_formats::pkg::Archive::default();
        for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
            let Ok(bytes) = std::fs::read(dir.join(name)) else {
                return;
            };
            archive.add(unbanished_formats::pkg::Package::parse(bytes).unwrap());
        }
        let mut ui = Ui::new(std::rc::Rc::new(archive));
        // The template puts the icons (`groupEffects`) in the window's title.
        let root = ui.group();
        let content = ui.instantiate(r"Dialog\StatusBar.rsc:content").unwrap();
        let effects = ui
            .instantiate(r"Dialog\StatusBar.rsc:groupEffects")
            .unwrap();
        ui.add_child(root, content);
        ui.add_child(root, effects);
        let mut bar = StatusBar::bind(&mut ui, root);
        let home = bar.status_images[0].unwrap();
        assert!(ui.node(home).flags & unbanished_formats::ui::flags::MOUSE != 0);
        let mut values = StatusValues {
            citizens: vec![
                (4, 0, true, 1),
                (5, 0, true, 2),
                (6, 0, true, 1),
                (7, 0, true, 0),
            ],
            ..StatusValues::default()
        };
        assert_eq!(bar.icon_click(&ui, home, &values), Some(4));
        assert_eq!(bar.icon_click(&ui, home, &values), Some(6));
        assert_eq!(bar.icon_click(&ui, home, &values), Some(4));
        let food = bar.status_images[1].unwrap();
        assert_eq!(bar.icon_click(&ui, food, &values), Some(5));
        values.game_flags = 0x400;
        assert_eq!(bar.icon_click(&ui, home, &values), None);
    }
}
