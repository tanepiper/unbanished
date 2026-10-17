//! Utility windows opened from the toolbar (Status Bar, Event Log, Professions, …), ported from
//! `FUN_140106f50` and the Runtime `UI::DialogInstance` constructor. See docs/UTILITY-WINDOWS.md.
//!
//! A utility template's `:ui` component is a `UIDescription`: a host dialog
//! (`Dialog/StandardDialog.rsc`) plus a table of `{controller type, element, slot name}`. The
//! element is instantiated into the named slot of the host and the controller is attached to it.
//! Clicking the toolbar button again closes the window; the window's upper-left corner is
//! remembered per template (default 0, 0).

use std::collections::BTreeMap;

use banished_formats::game::{external_list, profession, ui_description, UiDescription};
use banished_formats::pkg::Archive;
use banished_ui::{Kind, NodeId, Ui};

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

/// `MapUI` (vtable `0x140162490`): OnOpened `FUN_14006d3e0` shows the minimap's two images
/// (`Game +0x148`: terrain render `+0x8`, entity overlay `+0x10`) in `imageMap` and
/// `imageOverlay`; the overlay takes clicks (id 1000, `FUN_14006d340` → `FUN_14006d1b0`).
/// `frustum` is a `FrustumOutlineElement` (`FUN_14006c7f0`). Read-only.
pub struct MapUi {
    overlay: Option<NodeId>,
    frustum: Option<NodeId>,
}

impl MapUi {
    fn bind(ui: &mut Ui, root: NodeId, size: u32) -> MapUi {
        for (name, texture) in [
            ("imageMap", crate::loops::MINIMAP),
            ("imageOverlay", crate::loops::MINIMAP_OVERLAY),
        ] {
            if let Some(n) = ui.find(root, name) {
                let shown = match ui.node(n).kind {
                    Kind::Image { size, .. } => size,
                    _ => [0, 0],
                };
                let node = ui.node_mut(n);
                node.kind = Kind::Picture {
                    texture,
                    size: [size as i32; 2],
                };
                node.min_size = [
                    node.min_size[0].max(shown[0]),
                    node.min_size[1].max(shown[1]),
                ];
            }
        }
        MapUi {
            overlay: ui.find(root, "imageOverlay"),
            frustum: ui.find(root, "frustum"),
        }
    }

    /// `FUN_14006c7f0`: the camera's far corners on the ground, in element pixels
    /// (`x·W/n + W/2`, `(W - 1) - (y·W/n + H/2)`), joined 0-1, 1-3, 3-2, 2-0.
    fn update(&self, ui: &mut Ui, v: &StatusValues) {
        let (Some(f), Some(corners)) = (self.frustum, v.frustum) else {
            return;
        };
        let r = ui.node(f).rect;
        let (w, h) = ((r[2] - r[0]) as f32, (r[3] - r[1]) as f32);
        let scale = w / v.map_size.max(1) as f32;
        let p: Vec<[i32; 2]> = corners
            .iter()
            .map(|c| {
                let x = c[0] * scale + w * 0.5;
                let y = (w - 1.0) - (c[1] * scale + h * 0.5);
                [x as i32, y as i32]
            })
            .collect();
        if let Kind::Lines { lines, .. } = &mut ui.node_mut(f).kind {
            *lines = [(0, 1), (1, 3), (3, 2), (2, 0)]
                .iter()
                .map(|&(a, b)| [p[a][0], p[a][1], p[b][0], p[b][1]])
                .collect();
        }
    }

    /// `FUN_14006d1b0`: a click at element pixel (x, y) of a W×H overlay targets
    /// `((x - W/2)·n/W, (H/2 - y)·n/W)`.
    fn click(&self, ui: &Ui, at: (i32, i32), size: u32) -> Option<glam::Vec2> {
        let r = ui.node(self.overlay?).rect;
        let (x, y, w, h) = (at.0 - r[0], at.1 - r[1], r[2] - r[0], r[3] - r[1]);
        if x < 0 || x >= w || y < 0 || y >= h {
            return None;
        }
        let scale = size as f32 / w as f32;
        Some(glam::Vec2::new(
            (x - (w >> 1)) as f32 * scale,
            ((h >> 1) - y) as f32 * scale,
        ))
    }
}

/// `EventLogUI` (vtable `0x140161d20`): OnOpened `FUN_140067fa0`, refresh `FUN_140067da0`,
/// events `FUN_140067f30`. Rows show the newest entries last; previous/next (ids 100/101) scroll
/// one entry, bottom (102) returns to the newest. Read-only over the log.
pub struct EventLogUi {
    previous: Option<NodeId>,
    next: Option<NodeId>,
    bottom: Option<NodeId>,
    /// Per display row: text (`@0` parameter) and go-to button.
    rows: Vec<(Option<(NodeId, String)>, Option<NodeId>)>,
    /// `+0x48`: entries scrolled back from the newest.
    scroll: i32,
    /// `notifybuttons` children (ids `0x67 + i`): the log's notify flags.
    checks: Vec<NodeId>,
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

impl EventLogUi {
    /// OnOpened `FUN_140067fa0`. The checkboxes take the log's notify flags.
    fn bind(
        ui: &mut Ui,
        root: NodeId,
        log: Option<&banished_game::events::EventLog>,
    ) -> EventLogUi {
        let checks: Vec<NodeId> = ui
            .find(root, "notifybuttons")
            .map(|n| ui.node(n).children.clone())
            .unwrap_or_default();
        for (i, &c) in checks.iter().enumerate() {
            let on = log.map_or(i != 0, |l| l.notify[i]);
            ui.set_checked(c, on);
        }
        let mut rows = Vec::new();
        if let Some(display) = ui.find(root, "display") {
            if let Kind::Control(banished_ui::Control::Table { cells, rows: r, .. }) =
                &ui.node(display).kind
            {
                for y in 0..r.len() {
                    let cell = |x: usize| cells.iter().find(|c| c.0 == x && c.1 == y).map(|c| c.2);
                    let button = cell(0);
                    let text = cell(1).map(|t| (t, text_of(ui, t)));
                    rows.push((text, button));
                }
            }
        }
        EventLogUi {
            previous: ui.find(root, "buttonPrevious"),
            next: ui.find(root, "buttonNext"),
            bottom: ui.find(root, "buttonBottom"),
            rows,
            scroll: 0,
            checks,
        }
    }

    /// `FUN_140067f30`.
    fn click(
        &mut self,
        ui: &Ui,
        node: NodeId,
        log: &banished_game::events::EventLog,
    ) -> Option<UtilityClick> {
        if let Some(i) = self
            .checks
            .iter()
            .position(|&c| c == node || ui.descendant(c, node))
        {
            return Some(UtilityClick::Notify(i, ui.checked(self.checks[i])));
        }
        if let Some(r) = self
            .rows
            .iter()
            .position(|(_, b)| b.is_some_and(|b| b == node || ui.descendant(b, node)))
        {
            // Entry `n - scroll - rows + r`.
            let idx =
                log.entries.len() as i64 - self.scroll as i64 - self.rows.len() as i64 + r as i64;
            let cell = (idx >= 0)
                .then(|| log.entries.get(idx as usize))
                .flatten()
                .map_or(-1, |e| e.cell);
            return (cell >= 0)
                .then_some(UtilityClick::GoTo(cell))
                .or(Some(UtilityClick::Handled));
        }
        if Some(node) == self.previous {
            self.scroll += 1;
        } else if Some(node) == self.next {
            self.scroll -= 1;
        } else if Some(node) == self.bottom {
            self.scroll = 0;
        } else {
            return None;
        }
        if self.scroll < 0 {
            self.scroll = 0;
        }
        Some(UtilityClick::Handled)
    }

    /// `FUN_140067da0`.
    fn update(&self, ui: &mut Ui, log: &banished_game::events::EventLog) {
        let n = log.entries.len() as i64;
        let rows = self.rows.len() as i64;
        let base = (n - self.scroll as i64 - 1) - (rows - 1);
        for (r, (text, button)) in self.rows.iter().enumerate() {
            let idx = base + r as i64;
            let entry = (idx >= 0).then(|| log.entries.get(idx as usize)).flatten();
            if let Some((node, template)) = text {
                let line = entry.map(|e| format_event(ui, e)).unwrap_or_default();
                ui.set_text(*node, &template.replace("@0", &line));
                if let Some(e) = entry {
                    ui.node_mut(*node).color = e.color;
                }
            }
            if let Some(b) = button {
                // Element slot `+0x40` (enabled) with whether the entry has a cell
                // (`+0x18 >= 0`); an empty row's button is disabled.
                ui.node_mut(*b).enabled = entry.is_some_and(|e| e.cell >= 0);
            }
        }
        for b in [self.bottom, self.next].into_iter().flatten() {
            ui.node_mut(b).enabled = self.scroll > 0;
        }
    }
}

/// `FUN_14009a2d0`'s text: `FormatIndexed(events[type], season, year, @2, @3, @4)`.
pub fn format_event(ui: &mut Ui, e: &banished_game::events::EventEntry) -> String {
    if let Some(text) = &e.text {
        return text.clone();
    }
    let template = ui.res.string_at("Dialog/StringTable.rsc:events", e.kind);
    let season = ui
        .res
        .string_at("Dialog/StringTable.rsc:seasons", e.month as i32);
    let mut out = template;
    for (i, v) in [season, e.year.to_string()]
        .into_iter()
        .chain(e.args.iter().cloned())
        .enumerate()
    {
        out = out.replace(&format!("@{i}"), &v);
    }
    out
}

/// Game values shown by the Status Bar (`FUN_14007eda0` reads them from the town, calendar,
/// environment and stock managers).
#[derive(Debug, Clone, Default)]
pub struct StatusValues {
    /// The event log, for EventLogUI.
    pub events: Option<banished_game::events::EventLog>,
    /// MapUI: map size in cells and the camera's far corners on the ground (`FUN_14006cf10`).
    pub map_size: u32,
    pub frustum: Option<[[f32; 2]; 4]>,
    pub town_name: String,
    pub year: u32,
    /// 0..12.
    pub month: usize,
    /// Fahrenheit, as stored by the environment controller (`+0x18`).
    pub temperature: f32,
    /// Environment weather state (`+0x10`): 0 precipitation, 1 cloudy, 2 partly sunny, 3 sunny.
    pub weather: i32,
    pub show_celsius: bool,
    pub adults: u32,
    pub students: u32,
    pub children: u32,
    /// Citizens with each status bit set: home, food, cold, job, tool, disease.
    pub status: [u32; 6],
    /// Average raw Citizen values and the first CitizenDescription's scales (+0x70/+0xa4).
    pub health: f32,
    pub happiness: f32,
    pub health_max: f32,
    pub happiness_max: f32,
    /// Stock counts per resource category, in the order of the inventory (`+0x170`, stride 0x20):
    /// wood, stone, iron, firewood, tools, food, herbs, clothes, alcohol, coal, textile, then ten
    /// custom slots.
    pub stock: [i32; 21],
    /// The stock manager's limit per slot (`+0x170` entry `+8`), for ResourceLimitUI.
    pub stock_limits: [i32; 21],
    /// Adults (not students) per profession, in `Game/Profession/Profession.rsc:professionList`
    /// order (laborer first): the counts `FUN_14007ba70` takes from citizen `+0x1f0`.
    pub professions: Vec<u32>,
    /// The town's live citizens: (citizen, profession, grown and not at school, status icon
    /// mask), for ProfessionUI's goto buttons and StatusBarUI's icons.
    pub citizens: Vec<(usize, usize, bool, u32)>,
    /// The game's flags (`Game +0xd0`; bit 10, a tutorial).
    pub game_flags: u32,
    /// Per profession, the places its enabled workplaces offer (`numberAvailable`).
    pub profession_capacity: Vec<i32>,
}

/// `StatusBarUI` (vtable `0x140163c40`): bound in `FUN_14007f8a0`, refreshed by `FUN_14007eda0`.
pub(crate) struct StatusBar {
    title: Option<(NodeId, String)>,
    season: Option<NodeId>,
    population: Option<(NodeId, String)>,
    year: Option<NodeId>,
    temperature: Option<NodeId>,
    /// numberWood … numberTextile, then numberCustom0..9 (same order as `StatusValues::stock`).
    stock: Vec<Option<NodeId>>,
    status_images: [Option<NodeId>; 6],
    status_numbers: [Option<(NodeId, String)>; 6],
    /// Rain, Cloudy, PartlySunny, Sunny, Snow (controller offsets `+0x1a8..+0x1c8`).
    weather: [Option<NodeId>; 5],
    hearts: Option<NodeId>,
    stars: Option<NodeId>,
    /// `+0x100`: the citizen the status icons last went to.
    cursor: i32,
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

pub(crate) fn text_of(ui: &Ui, id: NodeId) -> String {
    match &ui.node(id).kind {
        Kind::Text(t) | Kind::Label { text: t, .. } => t.text.clone(),
        _ => String::new(),
    }
}

/// `UI::TextElement::SetParameter` applied to the element's original text.
pub(crate) fn set_parameters(ui: &mut Ui, (id, template): &(NodeId, String), values: &[String]) {
    let mut out = template.clone();
    for (i, v) in values.iter().enumerate() {
        out = out.replace(&format!("@{i}"), v);
    }
    ui.set_text(*id, &out);
}

impl StatusBar {
    fn bind(ui: &mut Ui, root: NodeId) -> StatusBar {
        let find = |ui: &Ui, name: &str| ui.find(root, name);
        let with_text = |ui: &Ui, name: &str| find(ui, name).map(|n| (n, text_of(ui, n)));
        let mut stock: Vec<_> = STOCK_NAMES.iter().map(|n| find(ui, n)).collect();
        stock.extend((0..10).map(|i| find(ui, &format!("numberCustom{i}"))));
        let status_images = STATUS_NAMES.map(|s| find(ui, &format!("imageStatus{s}")));
        // `FUN_14007f8a0` gives the six status icons element ids 100..105 and turns on flag
        // `0x10000` (`SetFlagsOnOff(0x10000, 0)`): each takes the mouse, a click its own event.
        for n in status_images.into_iter().flatten() {
            ui.node_mut(n).flags |= banished_formats::ui::flags::MOUSE;
        }
        let status_numbers = STATUS_NAMES.map(|s| with_text(ui, &format!("numberStatus{s}")));
        StatusBar {
            title: with_text(ui, "labelTitleParam"),
            season: find(ui, "textSeason"),
            population: with_text(ui, "textPopulation"),
            year: find(ui, "numberYear"),
            temperature: find(ui, "numberTemperature"),
            stock,
            status_images,
            status_numbers,
            weather: WEATHER_NAMES.map(|n| find(ui, n)),
            hearts: find(ui, "groupHearts"),
            stars: find(ui, "groupStars"),
            cursor: -1,
        }
    }

    /// A click on status icon `i` (element id `100 + i`; the handler at `0x14007f870` takes
    /// click events on ids 100..106) goes to `FUN_14007f5f0`: unless the game is a tutorial
    /// (flag bit 10), the citizens after the last one shown, round to the start, until one
    /// whose StatusIconComponent mask (`+0xf0`) has bit `i`; that one is shown and selected
    /// (`FUN_140108510`). The cursor keeps its place when none has it.
    fn icon_click(&mut self, ui: &Ui, node: NodeId, values: &StatusValues) -> Option<usize> {
        let i = self
            .status_images
            .iter()
            .position(|n| n.is_some_and(|n| n == node || ui.descendant(n, node)))?;
        if values.game_flags & 0x400 != 0 {
            return None;
        }
        let citizens = &values.citizens;
        let n = citizens.len() as i32;
        if n == 0 {
            return None;
        }
        let mask = 1u32 << i;
        self.cursor += 1;
        let mut tries = 0;
        loop {
            if n <= self.cursor {
                self.cursor = 0;
            }
            let c = &citizens[self.cursor as usize];
            if c.3 & mask != 0 || n <= tries {
                return (c.3 & mask != 0).then_some(c.0);
            }
            self.cursor += 1;
            tries += 1;
        }
    }

    /// `FUN_14007eda0` (`OnUpdateLayout`).
    fn update(&self, ui: &mut Ui, v: &StatusValues) {
        // Status icons and counts show only while some citizen has that status.
        for i in 0..6 {
            let on = v.status[i] > 0;
            if let Some(img) = self.status_images[i] {
                ui.set_visible(img, on);
            }
            if let Some(number) = &self.status_numbers[i] {
                ui.set_visible(number.0, on);
                set_parameters(ui, number, &[v.status[i].to_string()]);
            }
        }
        if let Some(p) = &self.population {
            let values = [v.adults, v.students, v.children].map(|n| n.to_string());
            set_parameters(ui, p, &values);
        }
        if let Some(year) = self.year {
            ui.set_text(year, &v.year.to_string());
        }
        if let Some(season) = self.season {
            let key = format!("Month{}", if v.month > 11 { 0 } else { v.month });
            let text = ui.res.string(Some(r"Dialog\StringTable.rsc:seasons"), &key);
            ui.set_text(season, &text);
        }
        let mut temperature = v.temperature;
        if v.show_celsius {
            temperature = (temperature - 32.0) * 5.0 / 9.0;
        }
        if let Some(t) = self.temperature {
            ui.set_text(t, &(temperature as i32).to_string());
        }
        // The converted value is compared with 32, as in the original.
        let mut state = v.weather;
        if temperature <= 32.0 && state == 0 {
            state = 4;
        }
        for (i, image) in self.weather.iter().enumerate() {
            if let Some(image) = *image {
                ui.set_visible(image, i as i32 == state);
            }
        }
        for (slot, node) in self.stock.iter().enumerate() {
            if let Some(node) = *node {
                ui.set_text(node, &v.stock[slot].to_string());
            }
        }
        Self::set_meter(ui, self.hearts, v.health, v.health_max);
        Self::set_meter(ui, self.stars, v.happiness, v.happiness_max);
        if let Some(title) = &self.title {
            set_parameters(ui, title, &[v.town_name.clone()]);
        }
    }

    /// FUN_14006b550 binds named whole/half images; FUN_14006b830 sets their visibility.
    pub(crate) fn set_meter(ui: &mut Ui, group: Option<NodeId>, value: f32, maximum: f32) {
        let Some(group) = group else { return };
        let icons = ui.node(group).children.clone();
        let values = banished_game::status::meter(value, maximum, icons.len());
        for (icon, (whole, half)) in icons.into_iter().zip(values) {
            if let Some(n) = ui.find(icon, "whole") {
                ui.set_visible(n, whole);
            }
            if let Some(n) = ui.find(icon, "half") {
                ui.set_visible(n, half);
            }
        }
    }
}

/// `ProfessionUI` (vtable `0x140163520`): one `professionRow` per profession, bound by
/// `FUN_14007c170` (OnOpened) and refreshed by `FUN_14007ba70`.
struct ProfessionUi {
    /// Per profession: the spinner (`spinnerCount`, ids 300 + i) and its number edit.
    spinners: Vec<Option<NodeId>>,
    /// Per profession: `numberAvailable`.
    available: Vec<Option<NodeId>>,
    /// Per profession: `buttonGoto` (ids 500 + i).
    gotos: Vec<Option<NodeId>>,
    /// `+0x70`: the citizen the goto buttons showed last.
    cursor: i32,
}

/// Set a spinner's value and its number text.
fn set_spinner(ui: &mut Ui, id: NodeId, value: i32) {
    let number = match &mut ui.node_mut(id).kind {
        Kind::Control(banished_ui::Control::Spinner {
            parts, value: v, ..
        }) => {
            *v = value;
            parts[0]
        }
        _ => None,
    };
    if let Some(n) = number {
        ui.set_text(n, &value.to_string());
    }
}

impl ProfessionUi {
    const ROW: &'static str = r"Dialog\Profession.rsc:professionRow";

    fn bind(ui: &mut Ui, root: NodeId, archive: &Archive) -> ProfessionUi {
        let list = crate::mods::profession_list(archive);
        let mut spinners = Vec::new();
        let mut available = Vec::new();
        let mut gotos = Vec::new();
        let Some(table) = ui.find(root, "professionTable") else {
            return ProfessionUi {
                spinners,
                available,
                gotos: Vec::new(),
                cursor: 0,
            };
        };
        // The table holds the row template; rows replace it (`TableElement::SetSize/SetChild`).
        ui.node_mut(table).children.clear();
        for (i, reference) in list.iter().enumerate() {
            let Some(p) = archive
                .read_ref(&reference.replace('\\', "/"))
                .ok()
                .and_then(|d| profession(&d).ok())
            else {
                spinners.push(None);
                available.push(None);
                gotos.push(None);
                continue;
            };
            let Some(row) = ui.instantiate(Self::ROW) else {
                spinners.push(None);
                available.push(None);
                gotos.push(None);
                continue;
            };
            ui.set_table_child(table, i, row);
            let table_ref = p.string_table.as_deref();
            if let Some(label) = ui.find(row, "labelProfession") {
                let text = ui.res.string(table_ref, &p.name);
                ui.set_text(label, &text);
                ui.node_mut(label).tooltip = ui.res.string(table_ref, &p.tooltip);
                if let Some(s) = ui.res.sprite(p.sprite_sheet.as_deref(), &p.name) {
                    if let Kind::Label { icon, .. } = &mut ui.node_mut(label).kind {
                        *icon = Some(s);
                    }
                }
            }
            let spinner = ui.find(row, "spinnerCount");
            // The laborer row (manager `+0x30`) has no workplaces: no "of N" and no goto button.
            if i == 0 {
                for name in ["ofText", "numberAvailable", "buttonGoto"] {
                    if let Some(n) = ui.find(row, name) {
                        ui.set_visible(n, false);
                    }
                }
            }
            spinners.push(spinner);
            gotos.push(if i == 0 {
                None
            } else {
                ui.find(row, "buttonGoto")
            });
            available.push(if i == 0 {
                None
            } else {
                ui.find(row, "numberAvailable")
            });
        }
        ProfessionUi {
            spinners,
            available,
            gotos,
            cursor: 0,
        }
    }

    /// The goto button (`500 + i`) under `node`: its profession.
    fn goto(&self, ui: &Ui, node: NodeId) -> Option<usize> {
        self.gotos
            .iter()
            .position(|g| g.is_some_and(|g| g == node || ui.descendant(g, node)))
    }

    /// `FUN_14007bf00`: with any working adult (grown, not at school), the next citizen after
    /// the last shown in the town's list with the profession; after as many misses as there
    /// are working adults, the one reached. `citizens` are the town's (citizen, profession,
    /// working adult).
    fn next_worker(
        &mut self,
        profession: usize,
        citizens: &[(usize, usize, bool, u32)],
    ) -> Option<usize> {
        let adults = citizens.iter().filter(|c| c.2).count() as i32;
        if adults == 0 {
            return None;
        }
        let n = citizens.len() as i32;
        self.cursor += 1;
        if self.cursor < 0 {
            self.cursor = 0;
        }
        let mut tries = 0;
        loop {
            if n <= self.cursor {
                self.cursor = 0;
            }
            let c = citizens[self.cursor as usize];
            if (c.1 == profession && c.2) || adults <= tries {
                return Some(c.0);
            }
            self.cursor += 1;
            tries += 1;
        }
    }

    /// A spinner changed: its profession and value, clamped as `FUN_14007ba70` sets the
    /// limits (0 to its count plus the laborers; the laborer spinner is fixed).
    fn changed(&self, ui: &Ui, node: NodeId, values: &StatusValues) -> Option<(usize, i32)> {
        let i = self
            .spinners
            .iter()
            .position(|s| s.is_some_and(|s| s == node || ui.descendant(s, node)))?;
        let s = self.spinners[i]?;
        let value = match &ui.node(s).kind {
            Kind::Control(banished_ui::Control::Spinner { value, .. }) => *value,
            _ => return None,
        };
        let count = |p: usize| values.professions.get(p).copied().unwrap_or(0) as i32;
        if i == 0 {
            return Some((0, count(0)));
        }
        Some((i, value.clamp(0, count(i) + count(0))))
    }

    /// `FUN_14007ba70`: worker counts; the laborer spinner is fixed, the others may take up to
    /// their count plus the laborers. Workplace capacity (`numberAvailable`) is 0 until
    /// workplaces exist.
    fn update(&self, ui: &mut Ui, values: &StatusValues) {
        let count = |p: usize| values.professions.get(p).copied().unwrap_or(0) as i32;
        for (i, spinner) in self.spinners.iter().enumerate() {
            if let Some(s) = *spinner {
                set_spinner(ui, s, count(i));
                // `NumberEditElement::SetMinMax`: the laborers' fixed, the others' 0 to their
                // count plus the laborers.
                let (lo, hi) = if i == 0 {
                    (count(0), count(0))
                } else {
                    (0, count(i) + count(0))
                };
                ui.set_spinner_range(s, lo, hi);
            }
            if let Some(n) = self.available.get(i).copied().flatten() {
                let v = values.profession_capacity.get(i).copied().unwrap_or(0);
                ui.set_text(n, &v.to_string());
            }
        }
    }
}

/// `HelpUI` (vtable `0x140162070`): bind `FUN_14006ae60`, refresh `FUN_14006ab90`, events
/// `FUN_14006abf0`. `comboTopic` (id `0xe74`) lists the help pages (`Game +0xa0`,
/// `Game\Help\HelpTopics.rsc`: string tables of Topic, Title and Text) by title; the chosen
/// page's text fills `documentHelp` (`0xe75`, `FUN_14006a880`), scrolled to the top, and joins
/// the history after the current page. A link opens the page whose topic it names
/// (`FUN_14006aaa0`); back (`0xe76`) and forward (`0xe77`) walk the history, each page keeping
/// where it was scrolled to; they are enabled when there is somewhere to go.
pub struct HelpUi {
    combo: Option<NodeId>,
    document: Option<NodeId>,
    container: Option<NodeId>,
    back: Option<NodeId>,
    forward: Option<NodeId>,
    /// The pages' string tables.
    pages: Vec<String>,
    /// `+0x68`: (page, scroll) visited; `+0x64` the current one.
    history: Vec<(usize, i32)>,
    at: usize,
}

impl HelpUi {
    fn bind(ui: &mut Ui, root: NodeId, archive: &Archive) -> HelpUi {
        let pages: Vec<String> = archive
            .read_ref("Game/Help/HelpTopics.rsc:resource")
            .ok()
            .and_then(|d| external_list(&d).ok())
            .unwrap_or_default();
        let document = ui.find(root, "documentHelp");
        // The container that scrolls the document.
        let container = (0..ui.nodes.len()).find(|&n| {
            ui.descendant(root, n)
                && matches!(&ui.node(n).kind, Kind::Control(banished_ui::Control::Container {
                    content: Some(c), ..
                }) if Some(*c) == document || document.is_some_and(|d| ui.descendant(*c, d)))
        });
        let combo = ui.find(root, "comboTopic");
        if let Some(c) = combo {
            let titles = pages
                .iter()
                .map(|t| ui.res.string(Some(&t.replace('\\', "/")), "Title"))
                .collect();
            ui.set_choices(c, titles);
        }
        let mut help = HelpUi {
            combo,
            document,
            container,
            back: ui.find(root, "buttonBack"),
            forward: ui.find(root, "buttonForward"),
            pages,
            history: Vec::new(),
            at: 0,
        };
        if let Some(c) = help.combo {
            crate::menu::select_combo(ui, c, 0);
        }
        help.open(ui, 0);
        help
    }

    fn scroll(&self, ui: &Ui) -> i32 {
        match self.container.map(|c| &ui.node(c).kind) {
            Some(Kind::Control(banished_ui::Control::Container { offset, .. })) => offset[1],
            _ => 0,
        }
    }

    fn set_scroll(&self, ui: &mut Ui, y: i32) {
        if let Some(c) = self.container {
            if let Kind::Control(banished_ui::Control::Container { offset, .. }) =
                &mut ui.node_mut(c).kind
            {
                offset[1] = y;
            }
        }
    }

    fn show(&self, ui: &mut Ui, page: usize) {
        let Some(table) = self.pages.get(page) else {
            return;
        };
        let text = ui.res.string(Some(&table.replace('\\', "/")), "Text");
        if let Some(d) = self.document {
            ui.set_text(d, &text);
        }
    }

    /// `FUN_14006a880`: the page, at the top, after the current one in the history.
    fn open(&mut self, ui: &mut Ui, page: usize) {
        self.show(ui, page);
        let scroll = self.scroll(ui);
        self.set_scroll(ui, 0);
        self.history.truncate(self.at + 1);
        self.history.push((page, scroll));
        self.at = self.history.len() - 1;
    }

    /// `FUN_14006ab90`.
    fn update(&self, ui: &mut Ui) {
        if let Some(b) = self.back {
            ui.node_mut(b).enabled = 0 < self.at;
        }
        if let Some(f) = self.forward {
            ui.node_mut(f).enabled = self.at + 1 < self.history.len();
        }
    }

    /// `FUN_14006abf0`: the topic's combo (event `0x14`), a link, back or forward.
    fn event(&mut self, ui: &mut Ui, event: banished_ui::Event) -> bool {
        use banished_ui::Event;
        match event {
            Event::Selected(n, i) if Some(n) == self.combo => {
                self.open(ui, i);
                true
            }
            Event::Link(n, i) if Some(n) == self.document => {
                let Some(target) = ui.document_link(n, i).map(str::to_owned) else {
                    return true;
                };
                let found = self
                    .pages
                    .iter()
                    .position(|t| ui.res.string(Some(&t.replace('\\', "/")), "Topic") == target);
                if let Some(page) = found {
                    let scroll = self.scroll(ui);
                    if let Some(h) = self.history.get_mut(self.at) {
                        h.1 = scroll;
                    }
                    if let Some(c) = self.combo {
                        crate::menu::select_combo(ui, c, page);
                    }
                    self.open(ui, page);
                }
                true
            }
            Event::Clicked(n) if Some(n) == self.back || Some(n) == self.forward => {
                let forward = Some(n) == self.forward;
                if (forward && self.at + 1 >= self.history.len()) || (!forward && self.at == 0) {
                    return true;
                }
                let scroll = self.scroll(ui);
                if let Some(h) = self.history.get_mut(self.at) {
                    h.1 = scroll;
                }
                self.at = if forward { self.at + 1 } else { self.at - 1 };
                let (page, scroll) = self.history[self.at];
                self.show(ui, page);
                self.set_scroll(ui, scroll);
                if let Some(c) = self.combo {
                    crate::menu::select_combo(ui, c, page);
                }
                true
            }
            _ => false,
        }
    }
}

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
    limits: Vec<crate::building_ui::ResourceLimitUi>,
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
        log: Option<&banished_game::events::EventLog>,
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
                crate::building_ui::RESOURCE_LIMIT_UI => {
                    limits.push(crate::building_ui::ResourceLimitUi::bind(
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
                .and_then(|d| banished_formats::game::toolbar_description(&d).ok())
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
        log: &banished_game::events::EventLog,
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
        if let Some(c) = self
            .open
            .iter()
            .flat_map(|w| &w.limits)
            .find_map(|l| l.click(ui, node))
        {
            if let crate::building_ui::WorkPlaceClick::StockLimit { slot, limit } = c {
                return Some(UtilityClick::StockLimit(slot, limit));
            }
        }
        self.open
            .iter_mut()
            .filter_map(|w| w.event_log.as_mut())
            .find_map(|e| e.click(ui, node, log))
    }

    /// An event for an open Help window (its combo, links and buttons).
    pub fn help_event(&mut self, ui: &mut Ui, event: banished_ui::Event) -> bool {
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
        let mut archive = banished_formats::pkg::Archive::default();
        for name in ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"] {
            let Ok(bytes) = std::fs::read(dir.join(name)) else {
                return;
            };
            archive.add(banished_formats::pkg::Package::parse(bytes).unwrap());
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
        assert!(ui.node(home).flags & banished_formats::ui::flags::MOUSE != 0);
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
