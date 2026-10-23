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

mod build;
mod refresh;

use unbanished_formats::pkg::Archive;
use unbanished_game::commerce::trade::{TradeGood, TradeSetting};
use unbanished_game::work::professions::Command;
use unbanished_ui::{Control, Kind, NodeId, Ui};

use crate::start::StartingTown;
use crate::ui::building_ui::{set_spinner, spinner_value, text_of, WorkPlaceClick};
use crate::ui::town_hall_ui::{combo_index, fill_table};

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
/// to 50, 20 up to 100; above that the increment stays as it was. The original's sell rows
/// get their most here only while the merchant is docked and buys the good.
///
/// Original: `FUN_140088250`.
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
        crate::ui::ui_settings::ints(&[
            combo_index(ui, self.sort) as i32,
            crate::ui::building_ui::tab_index(ui, self.tabs),
        ])
    }

    /// `FUN_14008bd90` reading: the tab and the sort; the list follows the sort
    /// (`FUN_140089b80`, at the next refresh).
    pub fn apply_settings(&mut self, ui: &mut Ui, settings: &[u8]) {
        let Some((version, v)) = crate::ui::ui_settings::read_ints(settings) else {
            return;
        };
        if version < 1 || v.len() < 2 {
            return;
        }
        crate::ui::building_ui::set_tab(ui, self.tabs, v[1]);
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
