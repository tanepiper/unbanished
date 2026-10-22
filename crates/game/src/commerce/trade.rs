//! Trade: TradeComponent (the trading post) and its merchants. See docs/TRADE.md.
//!
//! Now and then (more often in bigger towns) a merchant's boat comes up the river; while it is
//! docked the merchant's goods can be traded for the post's. A merchant brings what was ordered
//! from it, else lots of the goods it deals in.
//!
//! Original: `FUN_14005cd90`, `FUN_14005cfb0`.
mod deals;
mod visits;

pub use visits::{next_merchant, trade_row};

// vtable 0x140160df0

use unbanished_formats::game::{Merchant, TradeRow, ValueDescription};

use crate::scheduler::ticks;
use crate::town::Sim;
use crate::work::construction::Callbacks;

/// `DAT_140178934`.
const PERIOD: f32 = 300.0;
/// `DAT_140178944`: how long a merchant stays (s).
const STAY: f32 = 900.0;
/// `DAT_140178858`: ordered goods cost more.
const ORDERED: f32 = 1.25;
/// Event type 13: a merchant arrived.
const ARRIVED: i32 = 0xd;

/// A resolved TradeDescription.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TradeTemplate {
    // +0x30
    /// The boat template.
    pub boat: usize,
    /// The merchants.
    pub merchants: Vec<Merchant>,
    /// The rows of the trade window.
    pub rows: Vec<TradeRow>,
    // +0x38
    /// The categories' flags.
    pub categories: Vec<u32>,
    // +0x20; +0x28
    /// The categories' icon and name keys (the trade window's), in the description's sprite sheet
    /// and string table.
    pub labels: Vec<(String, String)>,
    /// The sprite sheet of the icons.
    pub sprites: Option<String>,
    /// The string table of the names.
    pub strings: Option<String>,
}

/// A good with a ValueDescription: its template, its raw material (none for seeds and
/// livestock) and value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TradeGood {
    /// The template.
    pub template: usize,
    /// The raw material; none for seeds and livestock.
    pub material: Option<usize>,
    /// The value.
    pub value: ValueDescription,
}

// +0x30
/// A merchant's order (TradeComponent, 0x20 bytes): the merchant's name, the goods ordered and its
/// state (0 none, 1 once, 2 every time).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Order {
    /// The merchant's name.
    pub name: String,
    /// The goods ordered.
    pub goods: Vec<usize>,
    /// The order's state: 0 none, 1 once, 2 every time.
    pub state: i32,
}

// +0x50
/// A good the merchant brought (0x10 bytes): the good, its value and how many.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Offer {
    /// The good.
    pub good: usize,
    /// Its value.
    pub value: i32,
    /// How many.
    pub amount: i32,
}

// +0x28; +0x2c; +0x30; +0x40; +0x50; +0x60; +0x70; +0x71; +0x78; +0x80
/// TradeComponent: when it trades by itself (0 never, 1 when a merchant arrives, 2 when it leaves),
/// the merchant docked (−1), the orders by merchant, the automatic trades by category (flags,
/// index, amount), the merchant's goods, the goods to keep at the post by raw material, TradeWork
/// on, held (the trade window is open), the time to the next merchant or until it leaves, the boat.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Trade {
    /// When it trades by itself: 0 never, 1 when a merchant arrives, 2 when it leaves.
    pub mode: i32,
    /// The merchant docked (−1 none).
    pub merchant: i32,
    /// The orders, by merchant.
    pub orders: Vec<Order>,
    /// The automatic trades by category: `(flags, index, amount)`.
    pub automatic: Vec<(u32, i32, i32)>,
    /// The merchant's goods.
    pub offers: Vec<Offer>,
    /// The goods to keep at the post, by raw material.
    pub keep: std::collections::BTreeMap<usize, i32>,
    /// Whether the post's hauling work is on.
    pub working: bool,
    /// Whether the trade window is open.
    pub held: bool,
    /// The time to the next merchant, or until it leaves.
    pub timer: i64,
    /// The boat, if any.
    pub boat: Option<usize>,
}

impl Trade {
    /// The activation: every merchant a name (`Rand(2)` picks the sex) and no order; every category
    /// no automatic trade.
    ///
    /// Original: `FUN_14005cb80`.
    pub fn new(
        t: &TradeTemplate,
        rng: &mut crate::random::Random,
        names: Option<&(
            unbanished_formats::names::NameDatabase,
            unbanished_formats::names::NameDatabase,
        )>,
    ) -> Trade {
        let orders = t
            .merchants
            .iter()
            .map(|_| {
                let male = rng.rand_below(2) == 0;
                let name = names.map_or(String::new(), |(m, f)| {
                    crate::people::names::citizen_name(if male { m } else { f }, rng)
                });
                Order {
                    name,
                    ..Default::default()
                }
            })
            .collect();
        Trade {
            merchant: -1,
            orders,
            automatic: t
                .categories
                .iter()
                .enumerate()
                .map(|(i, &f)| (f, i as i32, 0))
                .collect(),
            ..Default::default()
        }
    }
}

impl Sim<'_, '_> {
    fn merchant_list(&self, m: &Merchant) -> Vec<usize> {
        merchant_list(self.trade_goods, self.templates, self.materials, m)
    }
}

// +0x14; +0x18
/// What the merchant docked gives for one: with none, its value; else its value for what it buys,
/// three quarters for what it takes, else nothing.
///
/// Original: `FUN_14005ae80`.
pub fn sell_value(
    goods: &[TradeGood],
    materials: &[crate::work::stock::RawMaterial],
    tr: &Trade,
    d: Option<&TradeTemplate>,
    material: usize,
) -> i32 {
    let Some(good) = goods.iter().find(|g| g.material == Some(material)) else {
        return 0;
    };
    if tr.merchant < 0 {
        return good.value.value;
    }
    let Some(m) = d.and_then(|d| d.merchants.get(tr.merchant as usize)) else {
        return 0;
    };
    let flags = materials[material].flags;
    if m.flags.0 & flags != 0 {
        good.value.value
    } else if m.flags.1 & flags != 0 {
        (good.value.value as f32 * 0.75) as i32
    } else {
        0
    }
}

// +0x10
/// The merchant's own goods, then every good whose raw material has a flag it deals in, in the
/// factory's order.
///
/// Original: `FUN_140059fd0`.
pub fn merchant_list(
    goods: &[TradeGood],
    templates: &[crate::nature::seeding::Template],
    materials: &[crate::work::stock::RawMaterial],
    m: &Merchant,
) -> Vec<usize> {
    let mut list: Vec<usize> = m
        .items
        .iter()
        .flatten()
        .filter_map(|r| {
            goods
                .iter()
                .position(|g| templates[g.template].reference.eq_ignore_ascii_case(r))
        })
        .collect();
    for (i, g) in goods.iter().enumerate() {
        if g.material
            .is_some_and(|mat| materials[mat].flags & m.sells != 0)
        {
            list.push(i);
        }
    }
    list
}

/// What the trade window changes on the post's TradeComponent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TradeSetting {
    // +0x60
    /// Storage tab spinner `2000 + i`: what to keep at the post.
    ///
    /// Original: `FUN_140086f80`.
    Keep {
        /// The raw material.
        material: usize,
        /// The amount to keep.
        amount: i32,
    },
    /// Purchase tab up `5000 + i` / down `6000 + i`: two automatic trades change places.
    ///
    /// Original: `FUN_1400871d0`.
    Swap(usize, usize),
    /// Purchase tab spinner `7000 + i`: the automatic trade's amount.
    ///
    /// Original: `FUN_140087120`.
    Automatic {
        /// The automatic trade.
        index: usize,
        /// The amount.
        amount: i32,
    },
    // +0x28
    /// `comboPurchase` (`0x4e22`): when it trades by itself.
    ///
    /// Original: `FUN_140087090`.
    Mode(i32),
    /// `comboOrder` (`0x4e23`): the docked merchant's order state.
    ///
    /// Original: `FUN_140087350`.
    OrderState(i32),
    /// Order tab check `8000 + i`: a good ordered from the docked merchant.
    ///
    /// Original: `FUN_1400872a0`.
    Order {
        /// The good.
        good: usize,
        /// Whether it is ordered.
        on: bool,
    },
    // +0x71
    /// The window is open or closed.
    ///
    /// Original: `FUN_140087f40`, `FUN_140087ed0`.
    Held(bool),
}

// vtable 0x140167e30; +0x48
/// TradeWork: each hauler and what it carries to (more) or from (fewer) the post.
///
/// Original: `FUN_1400fc370`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TradeWork {
    /// Each hauler and what it carries: `(raw material, count)`.
    pub haulers: std::collections::BTreeMap<usize, (usize, i32)>,
}

impl Sim<'_, '_> {
    /// `FUN_140134130(work, 1, 0, TradeWork)`.
    pub(crate) fn set_trade_work(&mut self, jobs: &mut crate::work::jobs::Jobs, e: usize) {
        let work = crate::work::jobs::Work::new(
            e,
            self.rect_cell(e),
            self.professions.laborer,
            crate::work::jobs::WorkKind::Trade(TradeWork::default()),
        );
        self.set_work(jobs, e, work, 1, false);
    }

    /// [`sell_value`] at the post `e`.
    pub fn sell_value(&self, e: usize, material: usize) -> i32 {
        let Some(site) = self.sites.get(&e) else {
            return 0;
        };
        let Some(tr) = site.trade.as_ref() else {
            return 0;
        };
        sell_value(
            self.trade_goods,
            self.materials,
            tr,
            self.site_templates[site.template].trade.as_ref(),
            material,
        )
    }
}
