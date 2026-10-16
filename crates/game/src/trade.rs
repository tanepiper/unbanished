//! Trade: TradeComponent (the trading post, vtable `0x140160df0`, constructor `FUN_14005cd90`,
//! states `FUN_14005cfb0`) and its merchants. See docs/TRADE.md.
//!
//! Now and then (more often in bigger towns) a merchant's boat comes up the river; while it is
//! docked the merchant's goods can be traded for the post's. A merchant brings what was
//! ordered from it, else lots of the goods it deals in.

use banished_formats::game::{Merchant, TradeRow, ValueDescription};

use crate::construction::Callbacks;
use crate::scheduler::ticks;
use crate::town::Sim;

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
    /// `+0x30`: the boat template.
    pub boat: usize,
    pub merchants: Vec<Merchant>,
    pub rows: Vec<TradeRow>,
    /// `+0x38`: the categories' flags.
    pub categories: Vec<u32>,
    /// The categories' icon and name keys (the trade window's), in the description's sprite
    /// sheet (`+0x20`) and string table (`+0x28`).
    pub labels: Vec<(String, String)>,
    pub sprites: Option<String>,
    pub strings: Option<String>,
}

/// A good with a ValueDescription: its template, its raw material (none for seeds and
/// livestock) and value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TradeGood {
    pub template: usize,
    pub material: Option<usize>,
    pub value: ValueDescription,
}

/// A merchant's order (TradeComponent `+0x30`, 0x20 bytes): the merchant's name, the goods
/// ordered and its state (0 none, 1 once, 2 every time).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Order {
    pub name: String,
    pub goods: Vec<usize>,
    pub state: i32,
}

/// A good the merchant brought (`+0x50`, 0x10 bytes): the good, its value and how many.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Offer {
    pub good: usize,
    pub value: i32,
    pub amount: i32,
}

/// TradeComponent: `+0x28` when it trades by itself (0 never, 1 when a merchant arrives, 2
/// when it leaves), `+0x2c` the merchant docked (−1), `+0x30` the orders by merchant, `+0x40`
/// the automatic trades by category (flags, index, amount), `+0x50` the merchant's goods,
/// `+0x60` the goods to keep at the post by raw material, `+0x70` TradeWork on, `+0x71` held
/// (the trade window is open), `+0x78` the time to the next merchant or until it leaves,
/// `+0x80` the boat.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Trade {
    pub mode: i32,
    pub merchant: i32,
    pub orders: Vec<Order>,
    pub automatic: Vec<(u32, i32, i32)>,
    pub offers: Vec<Offer>,
    pub keep: std::collections::BTreeMap<usize, i32>,
    pub working: bool,
    pub held: bool,
    pub timer: i64,
    pub boat: Option<usize>,
}

impl Trade {
    /// The activation (`FUN_14005cb80`): every merchant a name (`Rand(2)` picks the sex) and
    /// no order; every category no automatic trade.
    pub fn new(
        t: &TradeTemplate,
        rng: &mut crate::random::Random,
        names: Option<&(
            banished_formats::names::NameDatabase,
            banished_formats::names::NameDatabase,
        )>,
    ) -> Trade {
        let orders = t
            .merchants
            .iter()
            .map(|_| {
                let male = rng.rand_below(2) == 0;
                let name = names.map_or(String::new(), |(m, f)| {
                    crate::names::citizen_name(if male { m } else { f }, rng)
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
    /// TradeComponent's activation (slot `+0x30`, `FUN_14005cb30`): the post joins the town's
    /// and its merchants get their names (`FUN_14005cb80`).
    pub(crate) fn trade_activate(&mut self, e: usize) {
        let Some(site) = self.sites.get(&e) else {
            return;
        };
        if site.trade.is_some() {
            return;
        }
        let Some(d) = self.site_templates[site.template].trade.clone() else {
            return;
        };
        self.trading_posts.push(e);
        let trade = Trade::new(&d, self.rng, self.names);
        self.sites.get_mut(&e).unwrap().trade = Some(trade);
    }

    fn trade_desc(&self, e: usize) -> Option<&TradeTemplate> {
        self.site_templates[self.sites.get(&e)?.template]
            .trade
            .as_ref()
    }

    pub(crate) fn trade(&mut self, e: usize) -> Option<&mut Trade> {
        self.sites.get_mut(&e)?.trade.as_mut()
    }

    /// `FUN_14005be50`: the first row whose population the town is below (else the last).
    fn trade_row(&self, d: &TradeTemplate) -> usize {
        d.rows
            .iter()
            .position(|r| 0 <= self.population && self.population < r.population)
            .unwrap_or(d.rows.len().saturating_sub(1))
    }

    /// State 2 enter (`FUN_14005b530`): with the post's workplace on and no work yet, its
    /// TradeWork; the time to the next merchant (`FUN_14005bdb0`: the row's months ±
    /// `RandFloatTol ×` its range, × 300 s).
    pub(crate) fn trade_enter2(&mut self, jobs: &mut crate::jobs::Jobs, e: usize) {
        let id = self.sites[&e].work_id;
        if !jobs.works.contains_key(&id)
            && self.workforce.workplaces.get(&e).is_some_and(|w| w.enabled)
        {
            self.set_trade_work(jobs, e);
        }
        let Some(d) = self.trade_desc(e).cloned() else {
            return;
        };
        let row = d.rows[self.trade_row(&d)];
        let f = self.rng.rand_float_tol();
        let t = ticks((f * row.range + row.months) * PERIOD);
        if let Some(tr) = self.trade(e) {
            tr.timer = t;
        }
    }

    /// State 2 update (`FUN_14005b3c0`): when the time is out, the merchant's boat sets out
    /// and the post waits for it (0x12).
    pub(crate) fn trade_update2(&mut self, e: usize, dt: i64) {
        let Some(tr) = self.trade(e) else { return };
        tr.timer -= dt;
        if 0 < tr.timer {
            return;
        }
        let Some(boat_t) = self.trade_desc(e).map(|d| d.boat) else {
            return;
        };
        let boat = self.add_boat(boat_t, e);
        if let Some(tr) = self.trade(e) {
            tr.boat = boat;
        }
        self.machine(e).request(0x12);
    }

    /// The transitions to 3 (`FUN_14005bac0`): the post leaves the town's; its boat sails off.
    pub(crate) fn trade_leave(&mut self, e: usize) {
        self.trading_posts.retain(|&p| p != e);
        if let Some(boat) = self.trade(e).and_then(|t| t.boat) {
            self.boat_orphan(boat);
            let tr = self.trade(e).unwrap();
            tr.offers.clear();
            tr.merchant = -1;
            tr.boat = None;
        }
    }

    /// State 0x13 enter (`FUN_14005b160`): the merchant's goods (`FUN_14005bee0`); it stays
    /// 900 s; trading by itself on arrival.
    pub(crate) fn trade_enter13(&mut self, e: usize) {
        self.merchant_goods(e);
        if let Some(tr) = self.trade(e) {
            tr.timer = ticks(STAY);
        }
        if self.trade(e).is_some_and(|t| t.mode == 1) {
            self.trade_automatically(e);
        }
    }

    /// State 0x13 exit (`FUN_14005b030`): trading by itself on leaving; the boat casts off.
    pub(crate) fn trade_exit13(&mut self, e: usize) {
        if self.trade(e).is_some_and(|t| t.mode == 2) {
            self.trade_automatically(e);
        }
        if let Some(boat) = self.trade(e).and_then(|t| t.boat) {
            self.boat_message(boat, 0);
        }
        let tr = self.trade(e).unwrap();
        tr.boat = None;
        tr.offers.clear();
        tr.merchant = -1;
    }

    /// State 0x13 update (`FUN_14005b0e0`): the stay runs out, unless the trade window holds it.
    pub(crate) fn trade_update13(&mut self, e: usize, dt: i64) {
        let Some(tr) = self.trade(e) else { return };
        tr.timer -= dt;
        if tr.timer < 1 && !tr.held {
            self.machine(e).request(2);
        }
    }

    /// The post's state messages: 9 in 0x12 (`FUN_14005b1b0`) the boat docked: event 13 at
    /// severity 2, → 0x13; 0xb in 0x12 (`FUN_14005b320`) it went without docking: → 2; 0xa in
    /// 0x13 (`FUN_14005bba0`) the merchant leaves now.
    pub fn trade_message(&mut self, e: usize, msg: i32) {
        if self.trade(e).is_none() || self.machine(e).index < 0 {
            return;
        }
        match (self.machine(e).state, msg) {
            (0x12, crate::boat::DOCKED_MESSAGE) => {
                self.site_events.push((ARRIVED, 2, e));
                self.machine(e).request(0x13);
            }
            (0x12, crate::boat::GONE_MESSAGE) => {
                if let Some(tr) = self.trade(e) {
                    tr.boat = None;
                }
                self.machine(e).request(2);
            }
            (0x13, 0xa) => self.machine(e).request(2),
            _ => {}
        }
    }

    /// `FUN_14005bee0`: a random merchant (`Rand(merchants)`); what was ordered from it, else
    /// the goods it deals in (`FUN_140059fd0`); a single order is used up. Then `merchant lots
    /// / the row's lot divisor` lots (at least one) of a random good: `max(lot / amount
    /// divisor, smallest lot)` (at least one) of it at its value (× 1.25 when ordered, at least
    /// one), added to its offer if it has one.
    fn merchant_goods(&mut self, e: usize) {
        let Some(d) = self.trade_desc(e).cloned() else {
            return;
        };
        let m = self.rng.rand_below(d.merchants.len() as u32) as usize;
        let row = d.rows[self.trade_row(&d)];
        let tr = self.trade(e).unwrap();
        tr.offers.clear();
        tr.merchant = m as i32;
        let order = tr.orders.get(m).cloned().unwrap_or_default();
        let ordered = order.state != 0 && !order.goods.is_empty();
        let list = if ordered {
            order.goods.clone()
        } else {
            self.merchant_list(&d.merchants[m])
        };
        if order.state != 2 {
            if let Some(o) = self.trade(e).unwrap().orders.get_mut(m) {
                o.goods.clear();
                o.state = 0;
            }
        }
        let lots = (d.merchants[m].lots / row.lot_divisor.max(1)).max(1);
        for _ in 0..lots {
            // Deviation: a merchant with nothing to sell brings nothing (the original reads
            // an empty list).
            if list.is_empty() {
                break;
            }
            let k = self.rng.rand_below(list.len() as u32) as usize;
            let good = list[k];
            let v = &self.trade_goods[good].value;
            let amount = (v.lot / row.amount_divisor.max(1)).max(v.min_lot).max(1);
            let mut value = v.value;
            if ordered {
                value = (value as f32 * ORDERED) as i32;
            }
            let value = value.max(1);
            let tr = self.trade(e).unwrap();
            match tr.offers.iter_mut().find(|o| o.good == good) {
                Some(o) => o.amount += amount,
                None => tr.offers.push(Offer {
                    good,
                    value,
                    amount,
                }),
            }
        }
    }

    fn merchant_list(&self, m: &Merchant) -> Vec<usize> {
        merchant_list(self.trade_goods, self.templates, self.materials, m)
    }

    /// `FUN_14005c960`: the TradeWork switched (slot `+0x70`, the post's work button): the
    /// workplace on with the work, or off without it.
    pub fn toggle_trade_work(&mut self, jobs: &mut crate::jobs::Jobs, e: usize) {
        let on = !self.workforce.workplaces.get(&e).is_some_and(|w| w.enabled);
        if let Some(tr) = self.trade(e) {
            tr.working = on;
        }
        if on {
            self.set_trade_work(jobs, e);
        } else {
            self.remove_work(jobs, e);
        }
        self.enable_workplace(e, on);
    }
}

/// `FUN_14005ae80`: what the merchant docked gives for one: with none, its value; else its
/// value for what it buys (`+0x14`), three quarters for what it takes (`+0x18`), else
/// nothing.
pub fn sell_value(
    goods: &[TradeGood],
    materials: &[crate::stock::RawMaterial],
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

/// `FUN_140059fd0`: the merchant's own goods, then every good whose raw material has a flag
/// it deals in (`+0x10`), in the factory's order.
pub fn merchant_list(
    goods: &[TradeGood],
    templates: &[crate::seeding::Template],
    materials: &[crate::stock::RawMaterial],
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeSetting {
    /// Storage tab spinner `2000 + i` (`FUN_140086f80`): `+0x60`, what to keep at the post.
    Keep { material: usize, amount: i32 },
    /// Purchase tab up `5000 + i` / down `6000 + i` (`FUN_1400871d0`): two automatic trades
    /// change places.
    Swap(usize, usize),
    /// Purchase tab spinner `7000 + i` (`FUN_140087120`): the automatic trade's amount.
    Automatic { index: usize, amount: i32 },
    /// `comboPurchase` (`0x4e22`, `FUN_140087090`): `+0x28`, when it trades by itself.
    Mode(i32),
    /// `comboOrder` (`0x4e23`, `FUN_140087350`): the docked merchant's order state.
    OrderState(i32),
    /// Order tab check `8000 + i` (`FUN_1400872a0`): a good ordered from the docked merchant.
    Order { good: usize, on: bool },
    /// `+0x71`: the window is open (`FUN_140087f40`) or closed (`FUN_140087ed0`).
    Held(bool),
}

/// TradeWork (vtable `0x140167e30`, constructor `FUN_1400fc370`): `+0x48` each hauler and what
/// it carries to (more) or from (fewer) the post.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TradeWork {
    pub haulers: std::collections::BTreeMap<usize, (usize, i32)>,
}

impl Sim<'_, '_> {
    /// `FUN_140134130(work, 1, 0, TradeWork)`.
    pub(crate) fn set_trade_work(&mut self, jobs: &mut crate::jobs::Jobs, e: usize) {
        let work = crate::jobs::Work::new(
            e,
            self.rect_cell(e),
            self.professions.laborer,
            crate::jobs::WorkKind::Trade(TradeWork::default()),
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

    /// `FUN_14005adf0`: trading brings the visitors' disease risk (TradeDescription `+0x68`
    /// for `+0x6c` months).
    fn trade_risk(&mut self, e: usize) {
        let Some(d) = self
            .sites
            .get(&e)
            .and_then(|s| self.site_templates[s.template].trade_risk)
        else {
            return;
        };
        if self.visitors.0 < d.0 {
            self.visitors.0 = d.0;
        }
        let time = ticks(d.1 as f32 * PERIOD);
        if self.visitors.1 < time {
            self.visitors.1 = time;
        }
    }

    /// `FUN_14005a430`: for every category with an amount set, the merchant's goods of that
    /// category are bought one at a time in turn (each fitting the post's storage) up to the
    /// amount, while what was bought is worth less than the post's goods; then the post's
    /// goods pay, item by item, one at a time at the merchant's value (goods it has no use for
    /// go for nothing). What was bought joins the post's storage.
    pub(crate) fn trade_automatically(&mut self, e: usize) {
        self.trade_risk(e);
        let storage = self.sites[&e].storage;
        let capacity = self.storages[storage].capacity;
        let mut used = self.storages[storage].used(self.materials);
        let mut bought: std::collections::BTreeMap<usize, i32> = Default::default();
        let requests = self
            .trade(e)
            .map(|t| t.automatic.clone())
            .unwrap_or_default();
        for (flags, _, want) in requests {
            if want == 0 {
                continue;
            }
            let wealth: i32 = self.storages[storage]
                .items
                .clone()
                .iter()
                .map(|i| self.sell_value(e, i.material) * (i.count - i.reserved))
                .sum();
            let offers = self.trade(e).unwrap().offers.clone();
            let mut candidates = Vec::new();
            let mut total = 0;
            for (k, o) in offers.iter().enumerate() {
                let good = &self.trade_goods[o.good];
                if good
                    .material
                    .is_some_and(|m| self.materials[m].flags & flags != 0)
                {
                    candidates.push(k);
                    total += o.amount;
                }
            }
            let (mut spent, mut count, mut idx) = (0, 0, 0);
            if !candidates.is_empty() {
                while count < want && spent < wealth {
                    let k = candidates[idx];
                    let o = self.trade(e).unwrap().offers[k];
                    if 0 < o.amount {
                        let m = self.trade_goods[o.good].material.unwrap();
                        used += self.materials[m].size;
                        if capacity < used {
                            break;
                        }
                        spent += o.value;
                        *bought.entry(o.good).or_default() += 1;
                        self.trade(e).unwrap().offers[k].amount -= 1;
                        count += 1;
                    }
                    idx += 1;
                    if candidates.len() <= idx {
                        idx = 0;
                    }
                    if total <= count {
                        break;
                    }
                }
            }
            // The post pays.
            let mut paid = 0;
            let mut k = 0;
            while k < self.storages[storage].items.len() {
                let item = self.storages[storage].items[k].clone();
                let v = self.sell_value(e, item.material);
                let n = item.count - item.reserved;
                let mut gone = 0;
                while paid < spent && gone < n {
                    paid += v;
                    gone += 1;
                    *self.trade_value += v;
                    // `FUN_14008e460`: each one traded away counts as used.
                    self.count_used(item.material, 1);
                    self.storages[storage].items[k].count -= 1;
                    used -= self.materials[item.material].size;
                }
                if self.storages[storage].items[k].count == 0 {
                    self.storages[storage].items.remove(k);
                } else {
                    k += 1;
                }
            }
        }
        for (good, n) in bought {
            if let Some(m) = self.trade_goods[good].material {
                self.storages[storage].add(m, n);
                self.discover(m);
            }
        }
    }

    /// `FUN_140097330`: the material joins the known list once.
    pub(crate) fn discover(&mut self, material: usize) {
        if !self.known_materials.contains(&material) {
            self.known_materials.push(material);
        }
    }

    /// TradeUI's trade button (`FUN_1400868f0`), while the merchant is docked: the visitors'
    /// disease risk; what is sold leaves the post (used) and its value counts as traded away;
    /// what is bought leaves the merchant's goods and arrives: raw materials in the post's
    /// storage (known), seeds known, livestock known and at the post (`FUN_14005b640`).
    pub fn exchange(&mut self, e: usize, sell: &[(usize, i32)], buy: &[(usize, i32)]) {
        if self.trade(e).is_none_or(|t| t.merchant < 0) {
            return;
        }
        self.trade_risk(e);
        let storage = self.sites[&e].storage;
        let mut value = 0;
        for &(m, n) in sell {
            if n == 0 {
                continue;
            }
            value += self.sell_value(e, m) * n;
            self.storages[storage].remove(m, n);
            self.count_used(m, n);
        }
        *self.trade_value += value;
        for &(good, n) in buy {
            if n == 0 {
                continue;
            }
            // `FUN_14005c400`: the offer shrinks, gone at none.
            let tr = self.trade(e).unwrap();
            if let Some(k) = tr.offers.iter().position(|o| o.good == good) {
                tr.offers[k].amount -= n;
                if tr.offers[k].amount == 0 {
                    tr.offers.remove(k);
                }
            }
            let template = self.trade_goods[good].template;
            if let Some(m) = self.trade_goods[good].material {
                self.storages[storage].add(m, n);
                self.discover(m);
            } else {
                if !self.known.contains(&template) {
                    self.known.push(template);
                }
                if self.wildlife.livestock_kinds.contains_key(&template) {
                    self.trade_livestock(e, template, n);
                }
            }
        }
    }

    /// `FUN_14005b640`'s area: the cells between the post's first two create points.
    fn trade_livestock(&mut self, e: usize, template: usize, n: i32) {
        let t = &self.templates[self.placed[e].template];
        let local = |i: usize| t.create_points.get(i).copied().unwrap_or_default();
        let (a, b) = (self.world_point(e, local(0)), self.world_point(e, local(1)));
        let size = self.size as i32;
        let half = (size >> 1) as f32;
        let cell = |p: [f32; 3]| {
            (
                ((p[0] + half) as i32).clamp(0, size - 1),
                ((p[1] + half) as i32).clamp(0, size - 1),
            )
        };
        let (c0, c1) = (cell(a), cell(b));
        let (x0, x1) = (c0.0.min(c1.0), c0.0.max(c1.0));
        let (y0, y1) = (c0.1.min(c1.1), c0.1.max(c1.1));
        let area = [
            x0 as i16,
            y0 as i16,
            (x1 - x0 + 1) as i16,
            (y1 - y0 + 1) as i16,
        ];
        self.with_wild(|w| w.trade_livestock(template, n, a, b, area));
    }

    /// TradeUI's settings (`FUN_140086f80`, `FUN_1400871d0`, `FUN_140087120`,
    /// `FUN_140087090`, `FUN_140087350`, `FUN_1400872a0`): see [`TradeSetting`].
    pub fn trade_setting(&mut self, e: usize, setting: TradeSetting) {
        let Some(tr) = self.trade(e) else { return };
        match setting {
            TradeSetting::Keep { material, amount } => {
                tr.keep.insert(material, amount);
            }
            TradeSetting::Swap(a, b) => {
                let n = tr.automatic.len();
                if a < n && b < n {
                    tr.automatic.swap(a, b);
                }
            }
            TradeSetting::Automatic { index, amount } => {
                if let Some(x) = tr.automatic.get_mut(index) {
                    x.2 = amount;
                }
            }
            TradeSetting::Mode(mode) => tr.mode = mode,
            TradeSetting::OrderState(state) => {
                let m = tr.merchant;
                if let Some(o) = usize::try_from(m).ok().and_then(|m| tr.orders.get_mut(m)) {
                    o.state = state;
                }
            }
            // `FUN_140059df0`: on adds the good (once), off takes it out.
            TradeSetting::Order { good, on } => {
                let m = tr.merchant;
                if let Some(o) = usize::try_from(m).ok().and_then(|m| tr.orders.get_mut(m)) {
                    let has = o.goods.contains(&good);
                    if !on || has {
                        o.goods.retain(|&g| g != good);
                    } else {
                        o.goods.push(good);
                    }
                }
            }
            TradeSetting::Held(held) => tr.held = held,
        }
    }

    /// TradeWork's slot `+0x20` (`FUN_1400fb210`): below its most workers, a hauler carrying
    /// something drops it off first. Then each raw material of the town's against what the
    /// post keeps (`+0x60`), what it holds and what is on its way: too much is carried to a
    /// storage that takes it (from a random use point); too little is fetched from a storage
    /// that has it to a random use point, as much as the hauler carries and the post takes.
    pub(crate) fn trade_try(&mut self, work: &mut crate::jobs::Work, c: usize) -> u8 {
        let e = work.entity;
        let crate::jobs::WorkKind::Trade(tw) = &work.kind else {
            return 0;
        };
        if self.work_max(e) <= tw.haulers.len() as i32 {
            return 1;
        }
        if 0 < self.states[c].carried.used(self.materials) {
            self.drop_off(c);
            return 2;
        }
        let storage = self.sites[&e].storage;
        let keep = self.trade(e).map(|t| t.keep.clone()).unwrap_or_default();
        let (mut fetch, mut take) = (Vec::new(), Vec::new());
        for m in 0..self.materials.len() {
            let want = keep.get(&m).copied().unwrap_or(0);
            let here = self.storages[storage].available(m);
            let crate::jobs::WorkKind::Trade(tw) = &work.kind else {
                return 0;
            };
            let moving: i32 = tw
                .haulers
                .values()
                .filter(|(mat, _)| *mat == m)
                .map(|(_, n)| n)
                .sum();
            let diff = want - moving - here;
            if 0 < diff {
                fetch.push((m, diff));
            } else if diff < 0 {
                take.push((m, -diff));
            }
        }
        let here = self.citizen_cell(c);
        let points = self.templates[self.placed[e].template].use_points.len();
        for (m, n) in take {
            let size = self.materials[m].size.max(1);
            let carried = &self.states[c].carried;
            let mut amount = n.min((carried.capacity - carried.used(self.materials)) / size);
            let avail = self.storages[storage].available(m);
            if avail == 0 {
                continue;
            }
            amount = amount.min(avail);
            let Some(found) = self.find_storage(m, amount, true, here) else {
                continue;
            };
            let k = self.rng.rand_below(points as u32) as usize;
            let source = self.use_point(e, k).0;
            let target_entity = self.storage_sites[found.site].entity;
            self.haul(
                c,
                work,
                crate::town::Haul {
                    target_entity,
                    target_storage: found.storage,
                    target: found.target,
                    source_storage: storage,
                    source_entity: e,
                    source,
                    material: m,
                    amount,
                    param: 0,
                    partial: true,
                    complete_on_pickup: false,
                    tool: true,
                    items: Vec::new(),
                },
            );
            self.took_job(c, work);
            if let crate::jobs::WorkKind::Trade(tw) = &mut work.kind {
                tw.haulers.insert(c, (m, -amount));
            }
            return 2;
        }
        for (m, n) in fetch {
            let Some(found) = self.find_storage(m, 1, false, here) else {
                continue;
            };
            let avail = self.storages[found.storage].available(m);
            if avail == 0 {
                continue;
            }
            let size = self.materials[m].size.max(1);
            let carried = &self.states[c].carried;
            let amount = n
                .min(avail)
                .min((carried.capacity - carried.used(self.materials)) / size);
            if !self.storages[storage].fits(self.materials, m, amount) {
                continue;
            }
            let k = self.rng.rand_below(points as u32) as usize;
            let target = self.use_point(e, k).0;
            let source_entity = self.storage_sites[found.site].entity;
            self.haul(
                c,
                work,
                crate::town::Haul {
                    target_entity: e,
                    target_storage: storage,
                    target,
                    source_storage: found.storage,
                    source_entity,
                    source: found.target,
                    material: m,
                    amount,
                    param: 0,
                    partial: false,
                    complete_on_pickup: false,
                    tool: true,
                    items: Vec::new(),
                },
            );
            self.took_job(c, work);
            if let crate::jobs::WorkKind::Trade(tw) = &mut work.kind {
                tw.haulers.insert(c, (m, amount));
            }
            return 2;
        }
        1
    }
}
