//! Searching for mods and showing the results.

use super::*;

impl Browse {
    pub(super) fn search(&mut self, ui: &mut Ui, settings: &Persistence) {
        self.search_with(ui, settings, None);
    }

    pub(super) fn search_with(
        &mut self,
        ui: &mut Ui,
        settings: &Persistence,
        mods: Option<&crate::mods::Modification>,
    ) {
        let task = match self.order {
            Order::Popular | Order::Recent => Task::Search {
                text: self.text.clone(),
                sort: if self.order == Order::Popular {
                    Sort::Downloads
                } else {
                    Sort::Updated
                },
                page: self.page,
            },
            Order::Subscribed => Task::Installed(self.installed(settings, mods)),
        };
        if let Task::Installed(ids) = &task {
            if ids.is_empty() {
                self.show_results(ui, Vec::new(), 0);
                return;
            }
        }
        self.searching = true;
        show(ui, self.working, true);
        show(ui, self.none, false);
        show(ui, self.error, false);
        self.nexus.start(task);
    }

    /// Put a page of results in the table (`FUN_14006dee0`): the page list, then one result
    /// group per mod, row by row.
    pub(super) fn show_results(&mut self, ui: &mut Ui, mods: Vec<ModSummary>, total: u32) {
        self.searching = false;
        show(ui, self.working, false);
        self.pages = total.div_ceil(PER_PAGE).max(1);
        if let Some(combo) = self.page_combo {
            ui.set_choices(combo, (1..=self.pages).map(|p| p.to_string()).collect());
            ui.select_combo_item(combo, self.page as usize);
        }
        if let Some(n) = self.prev {
            ui.node_mut(n).enabled = self.page > 0;
        }
        if let Some(n) = self.next {
            ui.node_mut(n).enabled = self.page + 1 < self.pages;
        }
        if let Some(table) = self.table {
            if let Kind::Control(Control::Table { cells, rows, .. }) = &mut ui.node_mut(table).kind
            {
                cells.clear();
                rows.clear();
            }
        }
        self.results.clear();
        show(ui, self.none, mods.is_empty());
        for (i, m) in mods.into_iter().enumerate() {
            let Some(group) = ui.instantiate(RESULT) else {
                continue;
            };
            if let Some(table) = self.table {
                ui.set_table_child(table, i, group);
            }
            let title = param(ui, group, "textModTitle");
            set_param(ui, &title, &m.name);
            if let Some(t) = title.map(|t| t.0) {
                ui.node_mut(t).tooltip = format!("{} (by {})\n{}", m.name, m.author, m.summary);
            }
            show(ui, ui.find(group, "buttonUnsubscribe"), false);
            // The loading image stands in for a preview until one arrives.
            let preview = ui.find(group, "imageModPreview");
            show(ui, preview, false);
            if let Some(url) = &m.thumbnail {
                self.nexus.start(Task::Preview {
                    mod_id: m.mod_id,
                    url: url.clone(),
                });
            }
            self.results.push(Result {
                summary: m,
                subscribe: ui.find(group, "buttonSubscribe"),
                preview,
            });
        }
    }

    /// A combo's choice (`Event::Selected`): the sort order or the page.
    pub fn selected(
        &mut self,
        ui: &mut Ui,
        settings: &Persistence,
        mods: &crate::mods::Modification,
        node: NodeId,
        index: usize,
    ) -> bool {
        let on = |n: Option<NodeId>| n.is_some_and(|b| ui.descendant(b, node));
        if on(self.sort) {
            self.order = match index {
                0 => Order::Popular,
                1 => Order::Recent,
                _ => Order::Subscribed,
            };
            self.page = 0;
            self.search_with(ui, settings, Some(mods));
            return true;
        }
        if on(self.page_combo) && index as u32 != self.page {
            self.page = index as u32;
            self.search_with(ui, settings, Some(mods));
            return true;
        }
        false
    }
}
