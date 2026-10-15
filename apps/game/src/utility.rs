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
}

/// Game values shown by the Status Bar (`FUN_14007eda0` reads them from the town, calendar,
/// environment and stock managers).
#[derive(Debug, Clone, Default)]
pub struct StatusValues {
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
    /// Adults (not students) per profession, in `Game/Profession/Profession.rsc:professionList`
    /// order (laborer first): the counts `FUN_14007ba70` takes from citizen `+0x1f0`.
    pub professions: Vec<u32>,
}

/// `StatusBarUI` (vtable `0x140163c40`): bound in `FUN_14007f8a0`, refreshed by `FUN_14007eda0`.
struct StatusBar {
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

fn text_of(ui: &Ui, id: NodeId) -> String {
    match &ui.node(id).kind {
        Kind::Text(t) | Kind::Label { text: t, .. } => t.text.clone(),
        _ => String::new(),
    }
}

/// `UI::TextElement::SetParameter` applied to the element's original text.
fn set_parameters(ui: &mut Ui, (id, template): &(NodeId, String), values: &[String]) {
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
        // `FUN_14007f8a0` gives the six status icons element ids 100..105 and clears flag 0x10000.
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
    fn set_meter(ui: &mut Ui, group: Option<NodeId>, value: f32, maximum: f32) {
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
        let list = archive
            .read_ref("Game/Profession/Profession.rsc:professionList")
            .ok()
            .and_then(|d| external_list(&d).ok())
            .unwrap_or_default();
        let mut spinners = Vec::new();
        let Some(table) = ui.find(root, "professionTable") else {
            return ProfessionUi { spinners };
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
                continue;
            };
            let Some(row) = ui.instantiate(Self::ROW) else {
                spinners.push(None);
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
        }
        ProfessionUi { spinners }
    }

    /// `FUN_14007ba70`: worker counts; the laborer spinner is fixed, the others may take up to
    /// their count plus the laborers. Workplace capacity (`numberAvailable`) is 0 until
    /// workplaces exist.
    fn update(&self, ui: &mut Ui, values: &StatusValues) {
        for (i, spinner) in self.spinners.iter().enumerate() {
            if let Some(s) = *spinner {
                set_spinner(
                    ui,
                    s,
                    values.professions.get(i).copied().unwrap_or(0) as i32,
                );
            }
        }
    }
}

struct Window {
    template: String,
    root: NodeId,
    status_bar: Option<StatusBar>,
    professions: Option<ProfessionUi>,
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
    pub fn toggle(&mut self, ui: &mut Ui, screen: &mut Screen, archive: &Archive, template: &str) {
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
                0 => {}
                controller::STATUS_BAR => status_bar = Some(StatusBar::bind(ui, root)),
                controller::PROFESSION => professions = Some(ProfessionUi::bind(ui, root, archive)),
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
        });
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

    /// `DialogInstance::OnUpdateLayout` for every open window's controllers.
    pub fn update(&self, ui: &mut Ui, values: &StatusValues) {
        for w in &self.open {
            if let Some(s) = &w.status_bar {
                s.update(ui, values);
            }
            if let Some(p) = &w.professions {
                p.update(ui, values);
            }
        }
    }
}
