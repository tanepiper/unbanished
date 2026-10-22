//! Trading: risk, automatic trades, discovering goods, the player's exchanges, livestock, the settings and the trader's work.

use super::*;

impl Sim<'_, '_> {
    /// `FUN_14005adf0`: trading brings the visitors' disease risk (TradeDescription `+0x68`
    /// for `+0x6c` months).
    pub(super) fn trade_risk(&mut self, e: usize) {
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

    /// TradeUI's trade button, while the merchant is docked: the visitors' disease risk; what is
    /// sold leaves the post (used) and its value counts as traded away; what is bought leaves the
    /// merchant's goods and arrives: raw materials in the post's storage (known), seeds known,
    /// livestock known and at the post.
    ///
    /// Original: `FUN_1400868f0`, `FUN_14005b640`.
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
    pub(super) fn trade_livestock(&mut self, e: usize, template: usize, n: i32) {
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

    /// TradeUI's settings: see [`TradeSetting`].
    ///
    /// Original: `FUN_140086f80`, `FUN_1400871d0`, `FUN_140087120`, `FUN_140087090`,
    /// `FUN_140087350`, `FUN_1400872a0`.
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
    pub(crate) fn trade_try(&mut self, work: &mut crate::work::jobs::Work, c: usize) -> u8 {
        let e = work.entity;
        let crate::work::jobs::WorkKind::Trade(tw) = &work.kind else {
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
            let crate::work::jobs::WorkKind::Trade(tw) = &work.kind else {
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
        let points = self.use_point_count(e);
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
            if let crate::work::jobs::WorkKind::Trade(tw) = &mut work.kind {
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
            if let crate::work::jobs::WorkKind::Trade(tw) = &mut work.kind {
                tw.haulers.insert(c, (m, amount));
            }
            return 2;
        }
        1
    }
}
