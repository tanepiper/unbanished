//! `TradeUI` (type `0xc8b87610`, vtable `0x1401642a0`: bind `FUN_14008a2a0`, open
//! `FUN_140087f40`, close `FUN_140087ed0`, refresh `FUN_140087620`, per-frame `FUN_140087c80`,
//! events `FUN_1400873f0`): the trading post's tabs (`Dialog\Trade.rsc:trade`). See
//! docs/BUILDING-WINDOWS.md and docs/TRADE.md.
//!
//! - Inventory (`tableStorage`, `FUN_140089b80`): every known raw material, its amount to keep
//!   at the post (spinner `2000 + i`, `FUN_140086f80`) and what the post holds; by name or by
//!   the amount to keep (`sortCombo`, id 199).
//! - Trade: the post's goods to keep (`tableSell`, `FUN_140089140`) at the docked merchant's
//!   value, how many to sell (spinner `3000 + i`, `FUN_140086f20`); the merchant's goods
//!   (`tableBuy`, `FUN_1400885d0`) and how many to buy (`4000 + i`, `FUN_140086ec0`); the
//!   totals (`FUN_140087df0`, `FUN_140087d20`), the trade (20000, `FUN_1400868f0`) and
//!   dismiss (`0x4e21`, `FUN_140086e20`) buttons.
//! - Purchase: when the post trades by itself (`comboPurchase`, `0x4e22`) and per category
//!   its order (`5000 + i` up, `6000 + i` down) and amount (`7000 + i`).
//! - Order: the docked merchant's order (`comboOrder`, `0x4e23`) and the goods it deals in
//!   (`tableOrder`, checks `8000 + i`).
//!
//! The amounts to sell and buy are the window's own (`+0x30`, `+0x40`) until traded.

use banished_formats::pkg::Archive;
use banished_game::professions::Command;
use banished_game::trade::{TradeGood, TradeSetting};
use banished_ui::{Control, Kind, NodeId, Ui};

use crate::building_ui::{set_spinner, spinner_value, text_of, WorkPlaceClick};
use crate::start::StartingTown;
use crate::town_hall_ui::{combo_index, fill_table};

/// The amount-to-keep spinners' most (`FUN_140089b80`, `SetMinMax(0, 9999)`).
const MOST: i32 = 9999;
/// `FUN_140087620`'s grey for goods the merchant has no use for.
const GREY: u32 = 0xff80_8080;
const WHITE: u32 = 0xffff_ffff;

struct Row {
    label: Option<NodeId>,
    stored: Option<NodeId>,
    value: Option<NodeId>,
    spinner: Option<NodeId>,
}

impl Row {
    fn find(ui: &Ui, cell: NodeId, value: &str, spinner: &str, stored: &str) -> Row {
        let f = |n: &str| ui.find(cell, n);
        Row {
            label: f("labelItem").or_else(|| f("labelStoreItem")),
            stored: f(stored),
            value: f(value),
            spinner: f(spinner),
        }
    }
}

struct PurchaseRow {
    up: Option<NodeId>,
    down: Option<NodeId>,
    label: Option<NodeId>,
    spinner: Option<NodeId>,
}

pub struct TradeUi {
    storage: Option<NodeId>,
    sell: Option<NodeId>,
    buy: Option<NodeId>,
    sell_total: Option<NodeId>,
    buy_total: Option<NodeId>,
    trade: Option<NodeId>,
    dismiss: Option<NodeId>,
    no_storage: Option<NodeId>,
    not_enough: Option<(NodeId, String)>,
    too_much: Option<(NodeId, String)>,
    purchase_combo: Option<NodeId>,
    order_combo: Option<NodeId>,
    sort: Option<NodeId>,
    /// `tradeTabs`.
    tabs: Option<NodeId>,
    names: Vec<(NodeId, String)>,
    purchase: Vec<PurchaseRow>,
    orders: Vec<(NodeId, Option<NodeId>)>,
    /// `+0x50`: the inventory's materials and rows.
    storage_rows: Vec<(usize, Row)>,
    /// `+0x30`: the goods to sell, how many, and rows.
    sell_rows: Vec<(usize, i32, Row)>,
    /// `+0x40`: the merchant's goods to buy, how many, and rows.
    buy_rows: Vec<(usize, i32, Row)>,
    /// `+0xb0`: the goods of the order tab.
    order_goods: Vec<usize>,
    /// What the tables were built from: known materials, kept materials, the merchant and its
    /// goods, the sort.
    built: Option<(Vec<usize>, Vec<usize>, i32, Vec<usize>, usize)>,
    /// The trade button was pressed: the amounts go back to none.
    traded: bool,
}

/// The good with `material` as its raw material.
fn good_of(goods: &[TradeGood], material: usize) -> Option<&TradeGood> {
    goods.iter().find(|g| g.material == Some(material))
}

fn set_label(ui: &mut Ui, label: Option<NodeId>, good: Option<&TradeGood>) {
    let Some(l) = label else { return };
    let (name, icon) = match good {
        Some(g) => (
            ui.res.string(g.value.name.0.as_deref(), &g.value.name.1),
            ui.res.sprite(g.value.icon.0.as_deref(), &g.value.icon.1),
        ),
        None => (String::new(), None),
    };
    if let Kind::Label { icon: i, .. } = &mut ui.node_mut(l).kind {
        *i = icon;
    }
    ui.set_text(l, &name);
}

fn set_number(ui: &mut Ui, n: Option<NodeId>, v: i32) {
    if let Some(n) = n {
        ui.set_text(n, &v.to_string());
    }
}

/// `SpinnerElement::SetMinMax(0, most)`, then `SetIncrement` by the most: 1 up to 20, 5 up
/// to 50, 20 up to 100; above that the increment stays as it was.
fn set_most(ui: &mut Ui, spinner: NodeId, most: i32) {
    ui.set_spinner_range(spinner, 0, most);
    let s = if most < 0x15 {
        1
    } else if most < 0x33 {
        5
    } else if most < 0x65 {
        20
    } else {
        return;
    };
    if let Kind::Control(Control::Spinner { step, .. }) = &mut ui.node_mut(spinner).kind {
        *step = s;
    }
}

fn set_enabled(ui: &mut Ui, n: Option<NodeId>, on: bool) {
    if let Some(n) = n {
        ui.node_mut(n).enabled = on;
    }
}

fn set_shown(ui: &mut Ui, n: Option<NodeId>, on: bool) {
    if let Some(n) = n {
        ui.set_visible(n, on);
    }
}

impl TradeUi {
    /// `FUN_14008bd90` writing: the sort, then the tab (version 1).
    pub fn settings(&self, ui: &Ui) -> Vec<u8> {
        crate::ui_settings::ints(&[
            combo_index(ui, self.sort) as i32,
            crate::building_ui::tab_index(ui, self.tabs),
        ])
    }

    /// `FUN_14008bd90` reading: the tab and the sort; the list follows the sort
    /// (`FUN_140089b80`, at the next refresh).
    pub fn apply_settings(&mut self, ui: &mut Ui, settings: &[u8]) {
        let Some((version, v)) = crate::ui_settings::read_ints(settings) else {
            return;
        };
        if version < 1 || v.len() < 2 {
            return;
        }
        crate::building_ui::set_tab(ui, self.tabs, v[1]);
        if let Some(c) = self.sort {
            crate::menu::select_combo(ui, c, v[0].max(0) as usize);
        }
    }

    pub fn bind(ui: &mut Ui, element: NodeId) -> TradeUi {
        let f = |n: &str| ui.find(element, n);
        let param = |n: &str| f(n).map(|id| (id, text_of(ui, id)));
        // The purchase table's cells by row: up and down, the category, its amount.
        let mut purchase: Vec<PurchaseRow> = Vec::new();
        if let Some(t) = f("tablePurchase") {
            let cells = match &ui.node(t).kind {
                Kind::Control(Control::Table { cells, .. }) => cells.clone(),
                _ => Vec::new(),
            };
            let rows = cells.iter().map(|c| c.1 + 1).max().unwrap_or(0);
            for r in 0..rows {
                let find = |n: &str| {
                    cells
                        .iter()
                        .filter(|c| c.1 == r)
                        .find_map(|c| ui.find(c.2, n))
                };
                purchase.push(PurchaseRow {
                    up: find("buttonMoveUp"),
                    down: find("buttonMoveDown"),
                    label: find("labelItemType"),
                    spinner: find("spinnerPurchaseDesired"),
                });
            }
        }
        let mut order_cells = match f("tableOrder").map(|t| &ui.node(t).kind) {
            Some(Kind::Control(Control::Table { cells, .. })) => cells.clone(),
            _ => Vec::new(),
        };
        order_cells.sort_by_key(|c| (c.1, c.0));
        let orders = order_cells
            .into_iter()
            .map(|c| (c.2, ui.find(c.2, "labelOrderItem")))
            .collect();
        let names = ["textName", "textOrder"]
            .iter()
            .filter_map(|n| param(n))
            .collect();
        TradeUi {
            storage: f("tableStorage"),
            sell: f("tableSell"),
            buy: f("tableBuy"),
            sell_total: f("numberSellTotal"),
            buy_total: f("numberBuyTotal"),
            trade: f("buttonTrade"),
            dismiss: f("buttonDismiss"),
            no_storage: f("textNoStorage"),
            not_enough: param("textNotEnough"),
            too_much: param("textTooMuch"),
            purchase_combo: f("comboPurchase"),
            order_combo: f("comboOrder"),
            sort: f("sortCombo"),
            tabs: f("tradeTabs"),
            names,
            purchase,
            orders,
            storage_rows: Vec::new(),
            sell_rows: Vec::new(),
            buy_rows: Vec::new(),
            order_goods: Vec::new(),
            built: None,
            traded: false,
        }
    }

    /// `FUN_140089b80`: every known raw material with a value, by name, or (`sortCombo` 1) by
    /// the amount to keep, most first, then by name.
    fn build_storage(&mut self, ui: &mut Ui, town: &StartingTown, keep: &dyn Fn(usize) -> i32) {
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
        // Deviation: a stable sort (the original's quicksort may order ties otherwise).
        list.sort_by(|a, b| {
            let order = if by_keep {
                keep(b.0).cmp(&keep(a.0))
            } else {
                std::cmp::Ordering::Equal
            };
            order.then_with(|| {
                if crate::building_ui::str_less(&a.1, &b.1) {
                    std::cmp::Ordering::Less
                } else if crate::building_ui::str_less(&b.1, &a.1) {
                    std::cmp::Ordering::Greater
                } else {
                    std::cmp::Ordering::Equal
                }
            })
        });
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
    fn build_sell(&mut self, ui: &mut Ui, town: &StartingTown, kept: &[usize]) {
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
    fn build_buy(&mut self, ui: &mut Ui, town: &StartingTown, e: usize) {
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
            banished_game::trade::merchant_list(
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
        let value =
            |m: usize| banished_game::trade::sell_value(&c.trade_goods, &town.materials, tr, d, m);
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

    /// `FUN_1400873f0` (events `0x12` clicked, `0x15` changed).
    pub fn click(&mut self, ui: &Ui, node: NodeId, e: usize) -> Option<WorkPlaceClick> {
        let hit = |n: Option<NodeId>| n.is_some_and(|n| n == node || ui.descendant(n, node));
        let setting = |setting| {
            Some(WorkPlaceClick::Command(Command::Trade {
                entity: e,
                setting,
            }))
        };
        if hit(self.trade) {
            self.traded = true;
            return Some(WorkPlaceClick::Command(Command::Exchange {
                entity: e,
                sell: self.sell_rows.iter().map(|r| (r.0, r.1)).collect(),
                buy: self.buy_rows.iter().map(|r| (r.0, r.1)).collect(),
            }));
        }
        if hit(self.dismiss) {
            // `FUN_140086e20`: the post's machine, message 10 (the merchant leaves).
            return Some(WorkPlaceClick::Command(Command::StateMessage {
                entity: e,
                message: 0xa,
            }));
        }
        for (m, row) in &self.storage_rows {
            if hit(row.spinner) {
                let amount = spinner_value(ui, row.spinner?)?.clamp(0, MOST);
                return setting(TradeSetting::Keep {
                    material: *m,
                    amount,
                });
            }
        }
        for (i, row) in self.purchase.iter().enumerate() {
            if hit(row.up) && 0 < i {
                return setting(TradeSetting::Swap(i, i - 1));
            }
            if hit(row.down) {
                return setting(TradeSetting::Swap(i, i + 1));
            }
            if hit(row.spinner) {
                let amount = spinner_value(ui, row.spinner?)?.clamp(0, MOST);
                return setting(TradeSetting::Automatic { index: i, amount });
            }
        }
        for (i, &(check, _)) in self.orders.iter().enumerate() {
            if hit(Some(check)) {
                let good = *self.order_goods.get(i)?;
                return setting(TradeSetting::Order {
                    good,
                    on: ui.checked(check),
                });
            }
        }
        None
    }

    /// The combos (event `0x14`): when the post trades by itself, the docked merchant's
    /// order; the inventory's sort is the window's own.
    pub fn select(&self, node: NodeId, index: usize, e: usize) -> Option<WorkPlaceClick> {
        let setting = |setting| {
            Some(WorkPlaceClick::Command(Command::Trade {
                entity: e,
                setting,
            }))
        };
        if self.purchase_combo == Some(node) {
            return setting(TradeSetting::Mode(index as i32));
        }
        if self.order_combo == Some(node) {
            return setting(TradeSetting::OrderState(index as i32));
        }
        None
    }
}
