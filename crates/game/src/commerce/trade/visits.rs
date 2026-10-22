//! A trading post's states: a merchant's arrival, stay and departure, its messages and the goods it brings.

use super::*;

// The rows are at TradeDescription +0x58 (count +0x60, 0x14 bytes each); the population is
// Game +0xf0 → +0xa8.
/// The trade row for a population: the first row whose population it is below, or the last row
/// (also when the population is below 0).
///
/// Measured: `tools/emu/trade_timer.py` (the original on 2,000 cases).
///
/// Original: `FUN_14005be50`.
pub fn trade_row(rows: &[TradeRow], population: i32) -> usize {
    rows.iter()
        .position(|r| 0 <= population && population < r.population)
        .unwrap_or(rows.len().saturating_sub(1))
}

// Row +0x4 months, +0x8 range; the result goes to TradeComponent +0x78.
/// The time to the next merchant: the row's months plus `RandFloatTol` × its range, × 300 s.
///
/// Measured: `tools/emu/trade_timer.py` (the original on 2,000 cases).
///
/// Original: `FUN_14005bdb0`.
pub fn next_merchant(row: &TradeRow, rng: &mut crate::random::Random) -> i64 {
    let f = rng.rand_float_tol();
    ticks((f * row.range + row.months) * PERIOD)
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

    pub(super) fn trade_desc(&self, e: usize) -> Option<&TradeTemplate> {
        self.site_templates[self.sites.get(&e)?.template]
            .trade
            .as_ref()
    }

    pub(crate) fn trade(&mut self, e: usize) -> Option<&mut Trade> {
        self.sites.get_mut(&e)?.trade.as_mut()
    }

    /// The trade row for the town's population now.
    ///
    /// Original: `FUN_14005be50`.
    pub(super) fn trade_row(&self, d: &TradeTemplate) -> usize {
        trade_row(&d.rows, self.population)
    }

    /// State 2 enter (`FUN_14005b530`): with the post's workplace on and no work yet, its
    /// TradeWork; then the time to the next merchant.
    pub(crate) fn trade_enter2(&mut self, jobs: &mut crate::work::jobs::Jobs, e: usize) {
        let id = self.sites[&e].work_id;
        if !jobs.works.contains_key(&id)
            && self.workforce.workplaces.get(&e).is_some_and(|w| w.enabled)
        {
            self.set_trade_work(jobs, e);
        }
        let Some(d) = self.trade_desc(e).cloned() else {
            return;
        };
        let t = next_merchant(&d.rows[self.trade_row(&d)], self.rng);
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

    /// The post's state messages: 9 in 0x12 the boat docked: event 13 at severity 2, → 0x13; 0xb in
    /// 0x12 it went without docking: → 2; 0xa in 0x13 the merchant leaves now.
    ///
    /// Original: `FUN_14005b1b0`, `FUN_14005b320`, `FUN_14005bba0`.
    pub fn trade_message(&mut self, e: usize, msg: i32) {
        if self.trade(e).is_none() || self.machine(e).index < 0 {
            return;
        }
        match (self.machine(e).state, msg) {
            (0x12, crate::commerce::boat::DOCKED_MESSAGE) => {
                self.site_events.push((ARRIVED, 2, e));
                self.machine(e).request(0x13);
            }
            (0x12, crate::commerce::boat::GONE_MESSAGE) => {
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
    pub(super) fn merchant_goods(&mut self, e: usize) {
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

    // slot +0x70
    /// The TradeWork switched (the post's work button): the workplace on with the work, or off
    /// without it.
    ///
    /// Original: `FUN_14005c960`.
    pub fn toggle_trade_work(&mut self, jobs: &mut crate::work::jobs::Jobs, e: usize) {
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
