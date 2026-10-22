//! Binding the town hall window, and its saved settings.

use super::*;

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
                        .and_then(|d| unbanished_formats::game::profession(&d).ok()),
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
}
