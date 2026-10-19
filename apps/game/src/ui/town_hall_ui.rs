//! `TownHallUI` (type `0xcd3b5953`, vtable slots: bind `FUN_1400864d0`, refresh
//! `FUN_140082a20`, per-frame `FUN_140083a20`, events `FUN_140081650`): the town hall's tabs
//! (`Dialog\TownHall.rsc:tabs`). See docs/BUILDING-WINDOWS.md.
//!
//! - Overview (`FUN_140085cc0` / `FUN_140082500`): the year and month, homes, families,
//!   citizens, adults, students, children, the shares clothed and educated, and the average
//!   health and happiness as hearts and stars.
//! - Professions (`FUN_140085630` / `FUN_1400833c0`): per profession its workers (a spinner,
//!   `0x1966 + i`), jobs, workplaces and a button cycling through them (`0x1999 + i`).
//! - Production (`FUN_140085100` / `FUN_140082180`): per stock slot the count, this year's
//!   use and production and the previous period's (`productionCombo`: 1 to 100 years).
//! - Inventory (`FUN_140084b30` / `FUN_140081cc0`, `FUN_140082a60`): every raw material held in
//!   storage, trading posts or homes (`inventoryCombo`), by name or by quantity (`sortCombo`).
//! - Graphs (`FUN_140084570` / `FUN_140083a80`): up to four series of the statistics over a
//!   time range (`timeCombo`, id `0x1964`; `graphCombo`, `0x1965`).
//! - Farming (`FUN_1400840c0` / `FUN_1400816f0`): the known crops, trees and livestock.

use banished_formats::pkg::Archive;
use banished_game::work::tracker::Statistics;
use banished_ui::{Control, Kind, NodeId, Ui};

use crate::start::StartingTown;
use crate::ui::building_ui::WorkPlaceClick;

/// What the town hall reads besides the town: the stock manager's slots and history.
pub struct TownContext<'a> {
    pub stock_counts: &'a [i32; 21],
    pub statistics: &'a Statistics,
}

/// One sample every 75 s: 48 a year.
const SAMPLES_PER_YEAR: i32 = 48;

fn text_of(ui: &Ui, id: NodeId) -> String {
    match &ui.node(id).kind {
        Kind::Text(t) => t.text.clone(),
        Kind::Label { text, .. } => text.text.clone(),
        Kind::Control(Control::Edit(e)) => e.text.text.clone(),
        _ => String::new(),
    }
}

fn set_number(ui: &mut Ui, n: Option<NodeId>, v: i32) {
    if let Some(n) = n {
        ui.set_text(n, &v.to_string());
    }
}

pub(crate) fn combo_index(ui: &Ui, combo: Option<NodeId>) -> usize {
    let Some(c) = combo else { return 0 };
    let list = match &ui.node(c).kind {
        Kind::Control(Control::Combo { list, .. }) => *list,
        _ => return 0,
    };
    match &ui.node(list).kind {
        Kind::List(l) => l.selected.unwrap_or(0),
        _ => 0,
    }
}

/// A table's cells, the first re-instantiated `count` times (`TableElement::SetSize` and a
/// clone of cell 0 per cell).
pub(crate) fn fill_table(ui: &mut Ui, table: NodeId, count: usize) -> Vec<NodeId> {
    let template = match &ui.node(table).kind {
        Kind::Control(Control::Table { cells, .. }) => cells.first().map(|c| c.2),
        _ => None,
    };
    let Some(reference) = template.map(|t| ui.node(t).reference.clone()) else {
        return Vec::new();
    };
    if let Kind::Control(Control::Table { cells, rows, .. }) = &mut ui.node_mut(table).kind {
        cells.clear();
        rows.clear();
    }
    let mut out = Vec::new();
    for i in 0..count {
        if let Some(cell) = ui.instantiate(&reference) {
            ui.set_table_child(table, i, cell);
            out.push(cell);
        }
    }
    out
}

struct ProfessionRow {
    spinner: Option<NodeId>,
    jobs: Option<NodeId>,
    locations: Option<NodeId>,
    goto: Option<NodeId>,
}

pub struct TownHallUi {
    /// `+0x30`: the tabs.
    tabs: Option<NodeId>,
    year: Option<NodeId>,
    month: Option<NodeId>,
    homes: Option<NodeId>,
    families: Option<NodeId>,
    citizens: Option<NodeId>,
    adults: Option<NodeId>,
    students: Option<NodeId>,
    children: Option<NodeId>,
    clothed: Option<(NodeId, String)>,
    educated: Option<(NodeId, String)>,
    hearts: Option<NodeId>,
    stars: Option<NodeId>,
    professions: Vec<ProfessionRow>,
    /// Per profession the workplace the goto button last showed.
    cursors: Vec<usize>,
    period: Option<NodeId>,
    prev_headers: [Option<(NodeId, String)>; 2],
    /// Per stock slot: count, used, made, previous used, previous made.
    limits: Vec<[Option<NodeId>; 5]>,
    inventory: Vec<(Option<NodeId>, Option<NodeId>)>,
    inventory_type: Option<NodeId>,
    sort: Option<NodeId>,
    graph: Option<NodeId>,
    time: Option<NodeId>,
    series: Option<NodeId>,
    x_min: Option<NodeId>,
    y_labels: [Option<NodeId>; 5],
    legends: [Option<NodeId>; 4],
    /// `+0x148`: the samples the graph was drawn with.
    drawn: Option<(usize, usize, usize)>,
    agri: Vec<NodeId>,
    filled: bool,
}

impl TownHallUi {
    /// `FUN_140086280` writing: the tab, then the period, inventory, sort, time and graph
    /// combos (version 1).
    pub fn settings(&self, ui: &Ui) -> Vec<u8> {
        let c = |n: Option<NodeId>| combo_index(ui, n) as i32;
        crate::ui::ui_settings::ints(&[
            crate::ui::building_ui::tab_index(ui, self.tabs),
            c(self.period),
            c(self.inventory_type),
            c(self.sort),
            c(self.time),
            c(self.series),
        ])
    }

    /// `FUN_140086280` reading: version 0 has no sort.
    pub fn apply_settings(&mut self, ui: &mut Ui, settings: &[u8]) {
        let Some((version, v)) = crate::ui::ui_settings::read_ints(settings) else {
            return;
        };
        let v: Vec<i32> = if version > 0 {
            v
        } else {
            let mut v = v;
            if v.len() >= 3 {
                v.insert(3, combo_index(ui, self.sort) as i32);
            }
            v
        };
        if v.len() < 6 {
            return;
        }
        crate::ui::building_ui::set_tab(ui, self.tabs, v[0]);
        for (combo, i) in [
            (self.period, v[1]),
            (self.inventory_type, v[2]),
            (self.sort, v[3]),
            (self.time, v[4]),
            (self.series, v[5]),
        ] {
            if let Some(c) = combo {
                crate::menu::select_combo(ui, c, i.max(0) as usize);
            }
        }
    }

    pub fn bind(ui: &mut Ui, archive: &Archive, element: NodeId) -> TownHallUi {
        let f = |ui: &Ui, n: &str| ui.find(element, n);
        let param = |ui: &Ui, n: &str| f(ui, n).map(|id| (id, text_of(ui, id)));
        // Professions: a row per profession.
        let list = crate::mods::profession_list(archive);
        let mut professions = Vec::new();
        if let Some(table) = f(ui, "tableProfessions") {
            let rows = fill_table(ui, table, list.len());
            for (i, (&row, reference)) in rows.iter().zip(&list).enumerate() {
                if let (Some(label), Some(p)) = (
                    ui.find(row, "labelProfession"),
                    archive
                        .read_ref(&reference.replace('\\', "/"))
                        .ok()
                        .and_then(|d| banished_formats::game::profession(&d).ok()),
                ) {
                    let t = p.string_table.as_deref();
                    let text = ui.res.string(t, &p.name);
                    ui.set_text(label, &text);
                    if let Some(s) = ui.res.sprite(p.sprite_sheet.as_deref(), &p.name) {
                        if let Kind::Label { icon, .. } = &mut ui.node_mut(label).kind {
                            *icon = Some(s);
                        }
                    }
                }
                let spinner = ui.find(row, "spinnerCount");
                professions.push(ProfessionRow {
                    spinner,
                    jobs: ui.find(row, "numberJobs"),
                    locations: ui.find(row, "numberLocations"),
                    goto: if i == 0 {
                        None
                    } else {
                        ui.find(row, "buttonGoto")
                    },
                });
                if i == 0 {
                    for n in ["numberJobs", "numberLocations", "buttonGoto"] {
                        if let Some(n) = ui.find(row, n) {
                            ui.set_visible(n, false);
                        }
                    }
                }
            }
        }
        // Production: a row per stock slot (its ResourceLimitUI in the first cell).
        let mut limits = Vec::new();
        if let Some(table) = f(ui, "limits") {
            let cells = ui.children(table);
            for row in cells.chunks(6) {
                let n = |k: usize| row.get(k).copied();
                limits.push([n(1), n(2), n(3), n(4), n(5)]);
            }
        }
        // The periods (`Dialog/StringTable.rsc:graphYears`).
        let period = f(ui, "productionCombo");
        if let Some(c) = period {
            let items = (0..7)
                .map(|i| ui.res.string_at("Dialog\\StringTable.rsc:graphYears", i))
                .collect();
            ui.set_choices(c, items);
            crate::menu::select_combo(ui, c, 0);
        }
        // Inventory: a cell per raw material.
        let mut inventory = Vec::new();
        let materials = crate::mods::raw_material_count(archive);
        if let Some(table) = f(ui, "inventoryTable") {
            for cell in fill_table(ui, table, materials) {
                inventory.push((ui.find(cell, "storageItem"), ui.find(cell, "storageCount")));
            }
        }
        let inventory_type = f(ui, "inventoryCombo");
        if let Some(c) = inventory_type {
            let items = (0..3)
                .map(|i| ui.res.string_at("Dialog\\StringTable.rsc:inventoryType", i))
                .collect();
            ui.set_choices(c, items);
            crate::menu::select_combo(ui, c, 0);
        }
        let sort = f(ui, "sortCombo");
        if let Some(c) = sort {
            crate::menu::select_combo(ui, c, 0);
        }
        // Graphs.
        let time = f(ui, "timeCombo");
        if let Some(c) = time {
            let items = (0..7)
                .map(|i| ui.res.string_at("Dialog\\StringTable.rsc:graphYears", i))
                .collect();
            ui.set_choices(c, items);
            crate::menu::select_combo(ui, c, 0);
        }
        let series = f(ui, "graphCombo");
        if let Some(c) = series {
            let items = (0..13)
                .map(|i| ui.res.string_at("Dialog\\StringTable.rsc:graphTypes", i))
                .collect();
            ui.set_choices(c, items);
            crate::menu::select_combo(ui, c, 0);
        }
        // Farming: one image per crop, tree and livestock kind.
        let agri = f(ui, "agriTable")
            .map(|t| ui.children(t))
            .unwrap_or_default();
        TownHallUi {
            tabs: f(ui, "tabs"),
            year: f(ui, "numberYear"),
            month: f(ui, "textMonth"),
            homes: f(ui, "numberHomes"),
            families: f(ui, "numberFamily"),
            citizens: f(ui, "numberCitizens"),
            adults: f(ui, "numberAdults"),
            students: f(ui, "numberStudents"),
            children: f(ui, "numberChildren"),
            clothed: param(ui, "percentClothed"),
            educated: param(ui, "percentEducated"),
            hearts: f(ui, "groupHearts"),
            stars: f(ui, "groupStars"),
            cursors: vec![0; professions.len()],
            professions,
            period,
            prev_headers: [
                param(ui, "limitPrevUsedHeader"),
                param(ui, "limitPrevProducedHeader"),
            ],
            limits,
            inventory,
            inventory_type,
            sort,
            graph: f(ui, "graphData"),
            time,
            series,
            x_min: f(ui, "textXMin"),
            y_labels: [
                f(ui, "number0"),
                f(ui, "number1"),
                f(ui, "number2"),
                f(ui, "number3"),
                f(ui, "number4"),
            ],
            legends: [
                f(ui, "label0"),
                f(ui, "label1"),
                f(ui, "label2"),
                f(ui, "label3"),
            ],
            drawn: None,
            agri,
            filled: false,
        }
    }

    pub fn refresh(
        &mut self,
        ui: &mut Ui,
        archive: &Archive,
        town: &StartingTown,
        ctx: &TownContext,
    ) {
        self.overview(ui, town);
        self.production(ui, town, ctx);
        self.inventory(ui, archive, town);
        if !self.filled {
            self.filled = true;
            self.agriculture(ui, archive, town);
        }
        // The per-frame slot (`FUN_140083a20`): the graph when a sample came in, the
        // professions.
        self.graph(ui, ctx.statistics, false);
        self.professions(ui, town);
    }

    /// `FUN_140082500`.
    fn overview(&mut self, ui: &mut Ui, town: &StartingTown) {
        let life = &town.agents.life;
        let (mut adults, mut students, mut children, mut families) = (0, 0, 0, 0);
        let (mut educated, mut clothed) = (0.0f32, 0.0f32);
        let (mut health, mut happiness) = (0.0f32, 0.0f32);
        let living: Vec<usize> = (0..town.citizens.len())
            .filter(|&c| town.citizens[c].alive)
            .collect();
        for &c in &living {
            let (citizen, s) = (&town.citizens[c], &town.agents.states[c]);
            if citizen.age < life.adult_age() || s.school.is_some() {
                if s.school.is_none() {
                    children += 1;
                } else {
                    students += 1;
                }
            } else {
                adults += 1;
                if life.i(0x118) <= citizen.education {
                    educated += 1.0;
                }
                // A family: an adult alone, or a couple counted by its man (`+0x250`, `+0x38`).
                match citizen.spouse {
                    None => families += 1,
                    Some(_) if citizen.male => families += 1,
                    _ => {}
                }
            }
            happiness += s.happiness;
            health += s.health;
            if s.clothing != 0 {
                clothed += 1.0;
            }
        }
        let n = living.len() as i32;
        let inverse = if 0 < n { 1.0 / n as f32 } else { 0.0 };
        if 0 < adults {
            educated = (educated / adults as f32) * 100.0;
        }
        set_number(ui, self.year, town.construction.year as i32);
        if let Some(m) = self.month {
            let month = ((town.construction.calendar / 3600.0) * 12.0) as i32;
            let month = if 11 < month { 0 } else { month };
            if let Some(table) = ui.node(m).string_table.clone() {
                let text = ui.res.string_at(&table, month);
                ui.set_text(m, &text);
            }
        }
        set_number(ui, self.homes, town.construction.homes.len() as i32);
        set_number(ui, self.citizens, n);
        set_number(ui, self.adults, adults);
        set_number(ui, self.students, students);
        set_number(ui, self.children, children);
        set_number(ui, self.families, families);
        if let Some((node, t)) = &self.clothed {
            let v = (clothed * inverse * 100.0 + 0.5) as i32;
            ui.set_text(*node, &t.replace("@0", &v.to_string()));
        }
        if let Some((node, t)) = &self.educated {
            let v = (educated + 0.5) as i32;
            ui.set_text(*node, &t.replace("@0", &v.to_string()));
        }
        if n < 1 {
            crate::ui::utility::StatusBar::set_meter(ui, self.hearts, 0.0, 1.0);
            crate::ui::utility::StatusBar::set_meter(ui, self.stars, 0.0, 1.0);
        } else {
            crate::ui::utility::StatusBar::set_meter(
                ui,
                self.hearts,
                health * inverse,
                life.f(0x70),
            );
            crate::ui::utility::StatusBar::set_meter(
                ui,
                self.stars,
                happiness * inverse,
                life.f(0xa4),
            );
        }
    }

    /// `FUN_1400833c0`: each profession's workers, jobs and workplaces.
    fn professions(&mut self, ui: &mut Ui, town: &StartingTown) {
        let (counts, places) = town.profession_counts();
        let registry = &town.construction.workforce.registry;
        let count = |p: usize| counts.get(p).copied().unwrap_or(0);
        for (i, row) in self.professions.iter().enumerate() {
            if let Some(s) = row.spinner {
                let v = count(i);
                if !ui.focused().is_some_and(|f| ui.descendant(s, f)) {
                    set_spinner(ui, s, v);
                }
                // `NumberEditElement::SetMinMax`, as the Professions window.
                let (lo, hi) = if i == 0 {
                    (count(0), count(0))
                } else {
                    (0, count(i) + count(0))
                };
                ui.set_spinner_range(s, lo, hi);
            }
            set_number(ui, row.jobs, places.get(i).copied().unwrap_or(0));
            set_number(
                ui,
                row.locations,
                registry.get(&i).map_or(0, |w| w.len() as i32),
            );
            if let Some(b) = row.goto {
                ui.node_mut(b).enabled = registry.get(&i).is_some_and(|w| !w.is_empty());
            }
        }
    }

    /// `FUN_140082180`: per slot its count, this year's use and production (the samples so
    /// far plus what is not sampled yet) and the previous period's.
    fn production(&mut self, ui: &mut Ui, town: &StartingTown, ctx: &TownContext) {
        let years = [1, 2, 5, 10, 25, 50, 100][combo_index(ui, self.period).min(6)];
        for (node, template) in self.prev_headers.iter().flatten() {
            ui.set_text(*node, &template.replace("@0", &years.to_string()));
        }
        let year = town.construction.year as i32;
        let start = year * SAMPLES_PER_YEAR - SAMPLES_PER_YEAR;
        let slice = ((town.construction.calendar / 3600.0) * SAMPLES_PER_YEAR as f32) as i32;
        let slice = if 0x2f < slice { 0 } else { slice };
        let now = start + slice;
        let (prev_from, prev_to) = (
            (year - years - 1) * SAMPLES_PER_YEAR,
            year * SAMPLES_PER_YEAR - SAMPLES_PER_YEAR - 1,
        );
        let counters = &town.construction.counters;
        for (k, row) in self.limits.iter().enumerate().take(21) {
            let s = ctx.statistics;
            let (used, made) = (9 + 3 * k, 10 + 3 * k);
            set_number(ui, row[0], ctx.stock_counts[k]);
            set_number(ui, row[1], sum(s, used, start, now) + counters.used[k]);
            set_number(ui, row[2], sum(s, made, start, now) + counters.made[k]);
            set_number(ui, row[3], sum(s, used, prev_from, prev_to));
            set_number(ui, row[4], sum(s, made, prev_from, prev_to));
        }
    }

    /// `FUN_140082a60` / `FUN_140081cc0`: every raw material held in storage (0), trading
    /// posts (1) or homes (2), by name or quantity.
    fn inventory(&mut self, ui: &mut Ui, archive: &Archive, town: &StartingTown) {
        let kind = combo_index(ui, self.inventory_type);
        let by_count = combo_index(ui, self.sort) == 1;
        let mut held = vec![0i32; town.materials.len()];
        let mut add = |storage: usize| {
            if let Some(s) = town.storages.get(storage) {
                for item in &s.items {
                    if let Some(h) = held.get_mut(item.material) {
                        *h += item.count;
                    }
                }
            }
        };
        match kind {
            0 => {
                for s in &town.storage_sites {
                    let trading = town
                        .construction
                        .sites
                        .get(&s.entity)
                        .is_some_and(|x| x.trade.is_some());
                    if !trading {
                        add(s.storage);
                    }
                }
            }
            1 => {
                for &p in &town.construction.trading_posts {
                    if let Some(site) = town.construction.sites.get(&p) {
                        add(site.storage);
                    }
                }
            }
            _ => {
                for home in town.construction.homes.values() {
                    add(home.storage);
                }
            }
        }
        let mut rows: Vec<(String, (Option<String>, String), i32)> = Vec::new();
        for (m, &count) in held.iter().enumerate() {
            let file = town.materials[m]
                .reference
                .split(':')
                .next()
                .unwrap_or("")
                .replace('\\', "/");
            let Some(v) = archive
                .read_ref(&format!("{file}:value"))
                .ok()
                .and_then(|d| banished_formats::game::value_description(&d).ok())
            else {
                continue;
            };
            let name = ui.res.string(v.name.0.as_deref(), &v.name.1);
            rows.push((name, v.icon.clone(), count));
        }
        // The CRT qsort: by quantity, most first, then name (`FUN_140015f10` with
        // `FUN_140081af0`), or by name (`FUN_140015c10` with `FUN_140081be0`).
        if by_count {
            banished_game::behaviour::ai::crt_qsort_less(&mut rows, |a, b| {
                b.2 < a.2 || (a.2 == b.2 && crate::ui::building_ui::str_less(&a.0, &b.0))
            });
        } else {
            banished_game::behaviour::ai::crt_qsort_less(&mut rows, |a, b| {
                crate::ui::building_ui::str_less(&a.0, &b.0)
            });
        }
        for (i, (label, number)) in self.inventory.iter().enumerate() {
            match rows.get(i) {
                Some((name, icon, count)) => {
                    if let Some(l) = *label {
                        let sprite = ui.res.sprite(icon.0.as_deref(), &icon.1);
                        if let Kind::Label { icon, .. } = &mut ui.node_mut(l).kind {
                            *icon = sprite;
                        }
                        ui.set_text(l, name);
                    }
                    set_number(ui, *number, *count);
                }
                None => {
                    if let Some(l) = *label {
                        if let Kind::Label { icon, .. } = &mut ui.node_mut(l).kind {
                            *icon = None;
                        }
                        ui.set_text(l, "");
                    }
                    if let Some(n) = *number {
                        ui.set_text(n, "");
                    }
                }
            }
        }
    }

    /// `FUN_140083a80`: the chosen series over the chosen time range, the y axis rounded up.
    fn graph(&mut self, ui: &mut Ui, statistics: &Statistics, force: bool) {
        let Some(g) = self.graph else { return };
        let time = combo_index(ui, self.time).min(6);
        let kind = combo_index(ui, self.series).min(22);
        let samples = statistics.series.first().map_or(0, |s| s.len());
        if !force && self.drawn == Some((samples, time, kind)) {
            return;
        }
        self.drawn = Some((samples, time, kind));
        // Each range's sample step, point count and x divisions (`SetXDivisions`, `+0xe4`).
        let (step, points, divisions) = [
            (1, 48, 12),
            (1, 96, 24),
            (2, 120, 5),
            (4, 120, 10),
            (8, 150, 25),
            (16, 150, 50),
            (32, 150, 100),
        ][time];
        // `AddGraph`'s colour for each of the (up to four) series, which its key's sprite
        // takes too (`LabelElement::SetSpriteColor`).
        const COLORS: [u32; 4] = [0xFFFF_FF00, 0xFF00_FF00, 0xFF80_80FF, 0xFFFF_0000];
        const SLOTS: [usize; 21] = [
            5, 0, 1, 2, 3, 9, 4, 6, 7, 8, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20,
        ];
        let series: Vec<usize> = match kind {
            0 => vec![0, 1, 2, 3],
            1 => vec![4, 6, 5, 7],
            k => vec![8 + 3 * SLOTS[(k - 2).min(20)]],
        };
        let points = points + 1;
        let mut top = 0;
        let mut lines = Vec::new();
        for &s in &series {
            let data = statistics.series.get(s).map_or(&[][..], |v| &v[..]);
            let have = data.len() as i32;
            let mut line = Vec::new();
            let mut from = have - points * step;
            if have / step < points {
                for _ in 0..points - have / step {
                    line.push(0);
                }
                from = 0;
            }
            let mut i = from.max(0);
            while i < have {
                let v = data[i as usize];
                top = top.max(v);
                line.push(v);
                i += step;
            }
            lines.push((COLORS[lines.len()], line));
        }
        let top = top.max(5);
        let unit = if top < 10 {
            1
        } else if top < 100 {
            10
        } else if top < 1000 {
            100
        } else {
            1000
        };
        let top = (top - 1 + unit) / unit * unit;
        ui.set_graph(g, top as f32, divisions, lines);
        for (i, label) in self.y_labels.iter().enumerate() {
            let v = top as f32 * (4 - i) as f32 * 0.25;
            set_number(ui, *label, v as i32);
        }
        for (i, legend) in self.legends.iter().enumerate() {
            let Some(l) = *legend else { continue };
            match series.get(i) {
                Some(&s) => {
                    let text = ui
                        .res
                        .string_at("Dialog\\StringTable.rsc:graphKeys", s as i32);
                    ui.set_text(l, &text);
                    ui.set_label_sprite_color(l, COLORS[i]);
                    ui.set_visible(l, true);
                }
                None => ui.set_visible(l, false),
            }
        }
        let _ = self.x_min;
    }

    /// `FUN_1400816f0`: the known crops, trees and livestock, each its SelectDescription's
    /// icon and name; the rest hidden.
    fn agriculture(&mut self, ui: &mut Ui, archive: &Archive, town: &StartingTown) {
        let mut known = Vec::new();
        for &t in &town.construction.known {
            let file = town.templates[t]
                .reference
                .split(':')
                .next()
                .unwrap_or("")
                .replace('\\', "/");
            if let Some(d) = archive
                .read_ref(&format!("{file}:select"))
                .ok()
                .and_then(|d| select_description(&d))
            {
                known.push(d);
            }
        }
        for (i, &cell) in self.agri.iter().enumerate() {
            match known.get(i) {
                Some((sheet, sprite, table, name)) => {
                    let s = ui.res.sprite(sheet.as_deref(), sprite);
                    if let Kind::Label { icon, .. } = &mut ui.node_mut(cell).kind {
                        *icon = s;
                    }
                    let text = ui.res.string(table.as_deref(), name);
                    ui.node_mut(cell).tooltip = text;
                    ui.set_visible(cell, true);
                }
                None => ui.set_visible(cell, false),
            }
        }
    }

    /// `FUN_140081650`: a spinner (`0x15`, `0x1966 + i`), a goto button (`0x12`,
    /// `0x1999 + i`) or a graph combo (`0x14`).
    pub fn click(&mut self, ui: &Ui, town: &StartingTown, node: NodeId) -> Option<WorkPlaceClick> {
        for (i, row) in self.professions.iter().enumerate() {
            if row
                .spinner
                .is_some_and(|s| s == node || ui.descendant(s, node))
            {
                let (counts, _) = town.profession_counts();
                let v = spinner_value(ui, row.spinner?)?;
                let free = counts.first().copied().unwrap_or(0);
                let count = counts.get(i).copied().unwrap_or(0);
                if i == 0 {
                    return None;
                }
                return Some(WorkPlaceClick::Command(
                    banished_game::work::professions::Command::SetCount {
                        profession: i,
                        count: v.clamp(0, count + free),
                    },
                ));
            }
            if row
                .goto
                .is_some_and(|b| b == node || ui.descendant(b, node))
            {
                // `FUN_140083170`: the next of the profession's workplaces.
                let list = town.construction.workforce.registry.get(&i)?;
                if list.is_empty() {
                    return None;
                }
                let k = self.cursors[i] % list.len();
                self.cursors[i] = k + 1;
                return Some(WorkPlaceClick::ShowBuilding(list[k]));
            }
        }
        None
    }

    /// A combo choice: the graph is drawn again.
    pub fn select(&mut self, ui: &mut Ui, statistics: &Statistics, node: NodeId) -> bool {
        if Some(node) == self.time || Some(node) == self.series {
            self.graph(ui, statistics, true);
            return true;
        }
        Some(node) == self.period || Some(node) == self.inventory_type || Some(node) == self.sort
    }
}

/// `FUN_1400963f0`: a series' sum over samples `from..=to`, clamped to what exists.
fn sum(s: &Statistics, series: usize, from: i32, to: i32) -> i32 {
    let Some(v) = s.series.get(series) else {
        return 0;
    };
    let from = from.max(0);
    let to = to.min(v.len() as i32 - 1);
    if to < from {
        return 0;
    }
    v[from as usize..=to as usize].iter().sum()
}

fn set_spinner(ui: &mut Ui, id: NodeId, value: i32) {
    let number = match &mut ui.node_mut(id).kind {
        Kind::Control(Control::Spinner {
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

fn spinner_value(ui: &Ui, id: NodeId) -> Option<i32> {
    match &ui.node(id).kind {
        Kind::Control(Control::Spinner { value, .. }) => Some(*value),
        _ => None,
    }
}

/// `SelectDescription` (serializer `FUN_140055f90`): `+0x20` sprite sheet, `+0x28` sprite,
/// `+0x30` string table, `+0x38` name.
fn select_description(data: &[u8]) -> Option<(Option<String>, String, Option<String>, String)> {
    use banished_formats::crs;
    crs::parse(data, |r: &mut crs::Reader, _| {
        r.block(|r, _| {
            let v = (
                r.external_ref()?,
                r.string()?,
                r.external_ref()?,
                r.string()?,
            );
            r.skip_rest();
            Ok(v)
        })
    })
    .ok()
    .map(|(_, v)| v)
}
