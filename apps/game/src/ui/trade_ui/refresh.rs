//! Refreshing the trade window from the town and the merchant.

use super::*;

impl TradeUi {
    /// `FUN_140087620` (and the per-frame `FUN_140087c80`): the tables rebuilt when what
    /// they show changed; the counts, values and totals; the notices; the buttons.
    pub fn refresh(&mut self, ui: &mut Ui, _archive: &Archive, town: &StartingTown, e: usize) {
        let c = &town.construction;
        let Some(site) = c.sites.get(&e) else { return };
        let Some(tr) = site.trade.as_ref() else {
            return;
        };
        let d = c.templates[site.template].trade.as_ref();
        let keep = |m: usize| tr.keep.get(&m).copied().unwrap_or(0);
        let kept: Vec<usize> = c
            .known_materials
            .iter()
            .copied()
            .filter(|&m| keep(m) != 0)
            .collect();
        let offers: Vec<usize> = tr.offers.iter().map(|o| o.good).collect();
        let sort = combo_index(ui, self.sort);
        let key = (
            c.known_materials.clone(),
            kept.clone(),
            tr.merchant,
            offers,
            sort,
        );
        let first = self.built.is_none();
        if self.built.as_ref() != Some(&key) {
            let old = self.built.take();
            if old
                .as_ref()
                .is_none_or(|o| o.0 != key.0 || o.1 != key.1 || o.4 != key.4)
            {
                self.build_storage(ui, town, &keep);
            }
            if old.as_ref().is_none_or(|o| o.1 != key.1 || o.2 != key.2) {
                self.build_sell(ui, town, &kept);
            }
            if old.as_ref().is_none_or(|o| o.2 != key.2 || o.3 != key.3) {
                self.build_buy(ui, town, e);
            }
            self.built = Some(key);
        }
        if first {
            // The bind: the purchase mode and the categories.
            if let Some(combo) = self.purchase_combo {
                crate::menu::select_combo(ui, combo, tr.mode.max(0) as usize);
            }
        }
        let storage = town.storages.get(site.storage);
        let available = |m: usize| storage.map_or(0, |s| s.available(m));
        let docked = site.machine.state == 0x13;
        if self.traded {
            self.traded = false;
            for r in self.sell_rows.iter_mut().chain(self.buy_rows.iter_mut()) {
                r.1 = 0;
                if let Some(s) = r.2.spinner {
                    set_spinner(ui, s, 0);
                }
            }
        }
        for (m, row) in &self.storage_rows {
            set_number(ui, row.stored, available(*m));
            if let Some(s) = row.spinner {
                let focused = ui.focused().is_some_and(|f| ui.descendant(s, f));
                if spinner_value(ui, s) != Some(keep(*m)) && !focused {
                    set_spinner(ui, s, keep(*m));
                }
            }
        }
        let value = |m: usize| {
            unbanished_game::commerce::trade::sell_value(&c.trade_goods, &town.materials, tr, d, m)
        };
        let mut sell_total = 0;
        let mut sold_size = 0;
        for (m, amount, row) in &mut self.sell_rows {
            let v = value(*m);
            let most = if docked && 0 < v { available(*m) } else { 0 };
            if let Some(s) = row.spinner {
                let n = spinner_value(ui, s).unwrap_or(0).clamp(0, most.max(0));
                if spinner_value(ui, s) != Some(n) {
                    set_spinner(ui, s, n);
                }
                set_most(ui, s, most);
                *amount = n;
                ui.node_mut(s).enabled = docked && 0 < v;
            }
            set_number(ui, row.value, v);
            set_number(ui, row.stored, available(*m));
            let color = if 0 < v { WHITE } else { GREY };
            for n in [row.label, row.value, row.stored].into_iter().flatten() {
                ui.node_mut(n).color = color;
            }
            sell_total += *amount * v;
            sold_size += *amount * town.materials[*m].size;
        }
        let mut buy_total = 0;
        let mut bought_size = 0;
        for (good, amount, row) in &mut self.buy_rows {
            let Some(o) = tr.offers.iter().find(|o| o.good == *good) else {
                continue;
            };
            if let Some(s) = row.spinner {
                let n = spinner_value(ui, s).unwrap_or(0).clamp(0, o.amount);
                if spinner_value(ui, s) != Some(n) {
                    set_spinner(ui, s, n);
                }
                *amount = n;
            }
            set_number(ui, row.stored, o.amount);
            buy_total += *amount * o.value;
            if let Some(m) = c.trade_goods[*good].material {
                bought_size += *amount * town.materials[m].size;
            }
        }
        set_number(ui, self.sell_total, sell_total);
        set_number(ui, self.buy_total, buy_total);
        // The notices: too little offered, no room for what is bought, or more offered than
        // needed.
        let not_enough = sell_total < buy_total;
        let used = storage.map_or(0, |s| s.used(&town.materials));
        let capacity = storage.map_or(0, |s| s.capacity);
        let no_room = capacity < used + (bought_size - sold_size);
        if let Some((n, t)) = &self.not_enough {
            ui.set_text(*n, &t.replace("@0", &(buy_total - sell_total).to_string()));
            ui.set_visible(*n, not_enough && !no_room);
        }
        set_shown(ui, self.no_storage, no_room);
        if let Some((n, t)) = &self.too_much {
            ui.set_text(*n, &t.replace("@0", &(sell_total - buy_total).to_string()));
            ui.set_visible(*n, !not_enough && !no_room && buy_total < sell_total);
        }
        set_enabled(ui, self.dismiss, docked);
        set_enabled(ui, self.trade, 0 < buy_total && !not_enough && !no_room);
        // The purchase tab: each category's amount, unless being typed.
        for (i, row) in self.purchase.iter().enumerate() {
            let Some(&(_, category, amount)) = tr.automatic.get(i) else {
                continue;
            };
            // `FUN_140087ff0`: the category's icon and name.
            if let (Some(l), Some(d)) = (row.label, d) {
                if let Some((icon, name)) = d.labels.get(category as usize) {
                    let sprite = ui.res.sprite(d.sprites.as_deref(), icon);
                    let text = ui.res.string(d.strings.as_deref(), name);
                    if let Kind::Label { icon, .. } = &mut ui.node_mut(l).kind {
                        *icon = sprite;
                    }
                    ui.set_text(l, &text);
                }
            }
            if let Some(s) = row.spinner {
                let focused = ui.focused().is_some_and(|f| ui.descendant(s, f));
                if spinner_value(ui, s) != Some(amount) && !focused {
                    set_spinner(ui, s, amount);
                }
            }
            set_enabled(ui, row.up, 0 < i);
            set_enabled(ui, row.down, i + 1 < tr.automatic.len());
        }
    }
}
