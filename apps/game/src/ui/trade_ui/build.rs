//! Building the trade window's rows: the storage, what to sell and what to buy.

use super::*;

impl TradeUi {
    /// `FUN_140089b80`: every known raw material with a value, by name, or (`sortCombo` 1) by
    /// the amount to keep, most first, then by name.
    pub(super) fn build_storage(
        &mut self,
        ui: &mut Ui,
        town: &StartingTown,
        keep: &dyn Fn(usize) -> i32,
    ) {
        let goods = &town.construction.trade_goods;
        let mut list: Vec<(usize, String)> = town
            .construction
            .known_materials
            .iter()
            .filter_map(|&m| {
                let g = good_of(goods, m)?;
                Some((m, ui.res.string(g.value.name.0.as_deref(), &g.value.name.1)))
            })
            .collect();
        let by_keep = combo_index(ui, self.sort) == 1;
        // The CRT qsort: by name (`FUN_140016ce0` with `FUN_140089aa0`) or by the amount to
        // keep, most first, then name (`FUN_140016fe0` with `FUN_140089900`).
        let name_less =
            |a: &(usize, String), b: &(usize, String)| crate::ui::building_ui::str_less(&a.1, &b.1);
        if by_keep {
            unbanished_game::behaviour::ai::crt_qsort_less(&mut list, |a, b| {
                let (ka, kb) = (keep(a.0), keep(b.0));
                kb < ka || (ka == kb && name_less(a, b))
            });
        } else {
            unbanished_game::behaviour::ai::crt_qsort_less(&mut list, name_less);
        }
        let cells = self
            .storage
            .map(|t| fill_table(ui, t, list.len()))
            .unwrap_or_default();
        self.storage_rows = list
            .iter()
            .zip(cells)
            .map(|(&(m, _), c)| {
                let row = Row::find(ui, c, "", "spinnerItemDesired", "numberItemStored");
                set_label(ui, row.label, good_of(goods, m));
                if let Some(s) = row.spinner {
                    set_spinner(ui, s, keep(m));
                    // `SetMinMax(0, 9999)` with no increment of its own.
                    ui.set_spinner_range(s, 0, MOST);
                }
                (m, row)
            })
            .collect();
    }

    /// `FUN_140089140`: the known materials with an amount to keep.
    pub(super) fn build_sell(&mut self, ui: &mut Ui, town: &StartingTown, kept: &[usize]) {
        let cells = self
            .sell
            .map(|t| fill_table(ui, t, kept.len()))
            .unwrap_or_default();
        self.sell_rows = kept
            .iter()
            .zip(cells)
            .map(|(&m, c)| {
                let row = Row::find(
                    ui,
                    c,
                    "numberItemSellValue",
                    "spinnerItemCount",
                    "numberItemStoredLarge",
                );
                set_label(ui, row.label, good_of(&town.construction.trade_goods, m));
                if let Some(s) = row.spinner {
                    set_spinner(ui, s, 0);
                }
                (m, 0, row)
            })
            .collect();
    }

    /// `FUN_1400885d0`: the merchant's goods; its name; the order tab's goods
    /// (`FUN_140059fd0`) and state.
    pub(super) fn build_buy(&mut self, ui: &mut Ui, town: &StartingTown, e: usize) {
        let c = &town.construction;
        let Some(site) = c.sites.get(&e) else { return };
        let Some(tr) = site.trade.as_ref() else {
            return;
        };
        let cells = self
            .buy
            .map(|t| fill_table(ui, t, tr.offers.len()))
            .unwrap_or_default();
        self.buy_rows = tr
            .offers
            .iter()
            .zip(cells)
            .map(|(o, cell)| {
                let row = Row::find(
                    ui,
                    cell,
                    "numberItemBuyValue",
                    "spinnerItemCount",
                    "numberItemStoredLarge",
                );
                set_label(ui, row.label, c.trade_goods.get(o.good));
                set_number(ui, row.value, o.value);
                set_number(ui, row.stored, o.amount);
                if let Some(s) = row.spinner {
                    set_spinner(ui, s, 0);
                    set_most(ui, s, o.amount);
                }
                (o.good, 0, row)
            })
            .collect();
        let d = c.templates[site.template].trade.as_ref();
        let merchant = usize::try_from(tr.merchant)
            .ok()
            .and_then(|m| Some((m, d?.merchants.get(m)?)));
        for (n, template) in &self.names {
            match merchant {
                Some((m, desc)) => {
                    let name = tr.orders.get(m).map_or("", |o| o.name.as_str());
                    let kind = ui.res.string(desc.name.0.as_deref(), &desc.name.1);
                    let text = template.replace("@0", name).replace("@1", &kind);
                    ui.set_text(*n, &text);
                }
                None => ui.set_text(*n, ""),
            }
        }
        self.order_goods = merchant.map_or(Vec::new(), |(_, desc)| {
            unbanished_game::commerce::trade::merchant_list(
                &c.trade_goods,
                &town.templates,
                &town.materials,
                desc,
            )
        });
        let ordered = merchant
            .and_then(|(m, _)| tr.orders.get(m))
            .cloned()
            .unwrap_or_default();
        for (i, &(check, label)) in self.orders.iter().enumerate() {
            let good = self.order_goods.get(i).copied();
            ui.set_visible(check, good.is_some());
            set_label(ui, label, good.and_then(|g| c.trade_goods.get(g)));
            ui.set_checked(check, good.is_some_and(|g| ordered.goods.contains(&g)));
        }
        if let Some(combo) = self.order_combo {
            crate::menu::select_combo(ui, combo, ordered.state.max(0) as usize);
            ui.node_mut(combo).enabled = merchant.is_some();
        }
    }
}
