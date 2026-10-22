//! The list of saves: refreshing and filling it.

use super::*;

impl SaveLoadDialog {
    /// `FUN_1400bbd50`.
    pub(super) fn refresh(&self, ui: &mut Ui) {
        let e = &self.el;
        let show = |ui: &mut Ui, n: Option<NodeId>, v: bool| {
            if let Some(n) = n {
                ui.set_visible(n, v);
            }
        };
        let usable = |ui: &mut Ui, n: Option<NodeId>, v: bool| {
            if let Some(n) = n {
                ui.node_mut(n).enabled = v;
            }
        };
        show(ui, e.label_load, self.loading);
        show(ui, e.label_save, !self.loading);
        show(ui, e.save, !self.loading);
        show(ui, e.load, self.loading);
        let idle = self.operation.is_none() && !self.listing && !self.deleting && !self.saved;
        if idle && self.load.is_none() {
            set_page(ui, e.page, 1);
            let readable = self
                .saves
                .get(self.selected.max(0) as usize)
                .filter(|_| self.selected >= 0)
                .is_some_and(|s| s.status == 2);
            usable(ui, e.load, readable);
            usable(ui, e.save, true);
            usable(ui, e.cancel, true);
            let file = self
                .saves
                .get(self.selected.max(0) as usize)
                .is_some_and(|s| s.file.is_some());
            usable(ui, e.delete, self.selected >= 0 && file);
            for (i, c) in self.cells.iter().enumerate() {
                let on = i as i32 == self.selected;
                show(ui, c.border, !on);
                show(ui, c.select, on);
            }
            return;
        }
        if let Some(n) = e.spinner {
            ui.node_mut(n).rotation = self.angle;
        }
        set_page(ui, e.page, 0);
        for n in [e.load, e.save, e.cancel, e.delete] {
            usable(ui, n, false);
        }
        let (wait, loading, saving) = match self.operation.map(|o| o.0) {
            Some(Operation::List) => (true, false, false),
            Some(Operation::Load) => (false, true, false),
            Some(Operation::Save) => (false, false, true),
            _ => return,
        };
        show(ui, e.wait, wait);
        show(ui, e.loading, loading);
        show(ui, e.saving, saving);
    }

    /// `FUN_1400bc1b0`: the table of saves.
    pub(super) fn fill(&mut self, ui: &mut Ui) {
        self.cells.clear();
        let Some(table) = self.el.table else { return };
        let count = self.saves.len();
        let cells = count.max(6).div_ceil(3) * 3;
        // `TableElement::SetSize(cells / 3 + 1, 3)`.
        if let Kind::Control(Control::Table {
            cells: c,
            rows,
            columns,
            ..
        }) = &mut ui.node_mut(table).kind
        {
            c.clear();
            rows.clear();
            rows.resize(cells / 3 + 1, 0);
            columns.clear();
            columns.resize(3, 0);
        }
        for i in 0..cells {
            let Some(cell) = ui.instantiate(CELL) else {
                continue;
            };
            ui.set_table_child(table, i, cell);
            let f = |ui: &Ui, n: &str| ui.find(cell, n);
            let (name, date, icon, border, select, corrupt) = (
                f(ui, "labelSaveName"),
                f(ui, "labelSaveDate"),
                f(ui, "imageSaveIcon"),
                f(ui, "borderSave"),
                f(ui, "selectSave"),
                f(ui, "imageCorrupt"),
            );
            let param = |ui: &mut Ui, n: Option<NodeId>, text: &str| {
                if let Some(n) = n {
                    let template = crate::ui::building_ui::text_of(ui, n);
                    ui.set_text(n, &template.replace("@0", text));
                }
            };
            let Some(info) = self.saves.get(i) else {
                // An unused cell.
                for n in [corrupt, select].into_iter().flatten() {
                    ui.set_visible(n, false);
                }
                if let Some(n) = border {
                    ui.node_mut(n).color = 0x80ff_ffff;
                }
                if let Some(n) = icon {
                    ui.node_mut(n).color = 0x20ff_ffff;
                }
                param(ui, name, "");
                param(ui, date, "");
                continue;
            };
            if let Some(n) = icon {
                ui.node_mut(n).flags |= unbanished_formats::ui::flags::MOUSE;
            }
            match &info.file {
                // The new-save slot: the save sprite, the template's own ("SaveLoadEmpty").
                None => {
                    if let Some(n) = corrupt {
                        ui.set_visible(n, false);
                    }
                    param(ui, name, "");
                    param(ui, date, "");
                }
                Some(file) => {
                    let (w, h) = (info.width.max(0) as u32, info.height.max(0) as u32);
                    let rgba = info
                        .image
                        .chunks(4)
                        .flat_map(|p| [p[2], p[1], p[0], 255])
                        .collect();
                    let sprite =
                        ui.res
                            .image_sheet(&format!("save:{file}:{}", info.date), w, h, rgba);
                    if let Some(n) = icon {
                        if let Kind::Image { sprite: s, .. } = &mut ui.node_mut(n).kind {
                            *s = Some(sprite);
                        }
                    }
                    let readable = info.status == 2;
                    if readable {
                        if let Some(n) = corrupt {
                            ui.set_visible(n, false);
                        }
                    } else if let Some(n) = icon {
                        ui.node_mut(n).color = 0xff80_8080;
                    }
                    let when = if readable {
                        save_list::date_string(info.date)
                    } else {
                        // `FUN_14009e670`: the game's names table (`+0xd8`).
                        ui.res.string(Some(GLOBAL_NAMES), "Corrupt")
                    };
                    param(ui, date, &when);
                    param(ui, name, &info.name.clone());
                }
            }
            self.cells.push(Cell {
                border,
                select,
                icon,
            });
        }
    }
}
