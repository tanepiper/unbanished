//! The status bar: the values it shows, its icons and meters.

use super::*;

/// Game values shown by the Status Bar (`FUN_14007eda0` reads them from the town, calendar,
/// environment and stock managers).
#[derive(Debug, Clone, Default)]
pub struct StatusValues {
    /// The event log, for EventLogUI.
    pub events: Option<unbanished_game::events::EventLog>,
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
    pub(super) status_images: [Option<NodeId>; 6],
    status_numbers: [Option<(NodeId, String)>; 6],
    /// Rain, Cloudy, PartlySunny, Sunny, Snow (controller offsets `+0x1a8..+0x1c8`).
    weather: [Option<NodeId>; 5],
    hearts: Option<NodeId>,
    stars: Option<NodeId>,
    /// `+0x100`: the citizen the status icons last went to.
    cursor: i32,
}

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
    pub(super) fn bind(ui: &mut Ui, root: NodeId) -> StatusBar {
        let find = |ui: &Ui, name: &str| ui.find(root, name);
        let with_text = |ui: &Ui, name: &str| find(ui, name).map(|n| (n, text_of(ui, n)));
        let mut stock: Vec<_> = STOCK_NAMES.iter().map(|n| find(ui, n)).collect();
        stock.extend((0..10).map(|i| find(ui, &format!("numberCustom{i}"))));
        let status_images = STATUS_NAMES.map(|s| find(ui, &format!("imageStatus{s}")));
        // `FUN_14007f8a0` gives the six status icons element ids 100..105 and turns on flag
        // `0x10000` (`SetFlagsOnOff(0x10000, 0)`): each takes the mouse, a click its own event.
        for n in status_images.into_iter().flatten() {
            ui.node_mut(n).flags |= unbanished_formats::ui::flags::MOUSE;
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
    pub(super) fn icon_click(
        &mut self,
        ui: &Ui,
        node: NodeId,
        values: &StatusValues,
    ) -> Option<usize> {
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
    pub(super) fn update(&self, ui: &mut Ui, v: &StatusValues) {
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
            set_parameters(ui, title, std::slice::from_ref(&v.town_name));
        }
    }

    /// FUN_14006b550 binds named whole/half images; FUN_14006b830 sets their visibility.
    pub(crate) fn set_meter(ui: &mut Ui, group: Option<NodeId>, value: f32, maximum: f32) {
        let Some(group) = group else { return };
        let icons = ui.node(group).children.clone();
        let values = unbanished_game::people::status::meter(value, maximum, icons.len());
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
