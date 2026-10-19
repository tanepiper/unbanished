//! The job manager's work slots ([`WorkHost`]): offering, assigning, finishing and cancelling work.

use super::*;

impl WorkHost for Sim<'_, '_> {
    fn profession(&self, c: usize) -> usize {
        self.states[c].profession
    }
    fn workplace(&self, c: usize) -> Option<usize> {
        self.states[c].workplace
    }
    fn cell(&self, c: usize) -> i32 {
        self.citizen_cell(c)
    }
    fn has_emergency(&self, c: usize) -> bool {
        let s = &self.states[c];
        crate::disasters::fire::has_emergency(
            self.cells,
            self.placed,
            s.home,
            s.workplace,
            self.characters[c].position,
        )
    }
    fn works_anywhere(&self, p: usize) -> bool {
        self.professions.works_anywhere(p)
    }
    fn rank(&mut self, a: i32, b: i32) -> i32 {
        self.rank_or_distance(a, b)
    }
    fn stock(&self, slot: usize) -> (i32, i32) {
        self.stock[slot]
    }
    fn laborer(&self) -> usize {
        self.professions.laborer
    }
    fn builder(&self) -> usize {
        self.professions.builder
    }
    fn rng(&mut self) -> &mut Random {
        self.rng
    }
    fn take_events(&mut self) -> Vec<JobEvent> {
        std::mem::take(&mut self.events)
    }

    /// `FUN_1400f1020` / `FUN_1400ea0b0`.
    fn refresh(&mut self, work: &mut Work) {
        let id = work.id;
        let gone: Vec<usize> = match &work.kind {
            WorkKind::Gather(g) => g.workers.iter().copied(),
            WorkKind::Build(b) => b.workers.iter().copied(),
            WorkKind::Clear(_) => return self.clear_refresh(work),
            WorkKind::Pickup(_) => return self.pickup_refresh(work),
            WorkKind::Produce(_) => return self.produce_refresh(work),
            WorkKind::Fishing(_) => return self.fishing_refresh(work),
            WorkKind::PlantArea(_) | WorkKind::Harvest(_) | WorkKind::PickFruit(_) => {
                return self.area_refresh(work)
            }
            WorkKind::PlantOrchard(_) => return self.plant_orchard_refresh(work),
            // `FUN_1400f3620` / `FUN_1400f0130`.
            WorkKind::Herbalist(h)
            | WorkKind::Forester(h)
            | WorkKind::Hunt(h)
            | WorkKind::TendArea(h)
            | WorkKind::Pasture(h)
            | WorkKind::ClearStorage(h)
            | WorkKind::Fire(h) => h.workers.iter().copied(),
            // `FUN_1400e9590`: the builder no longer on it.
            WorkKind::Road(r) => {
                if let Some(c) = r.builder {
                    if self.characters[c].stack.holds_task(id).is_none() {
                        if let WorkKind::Road(r) = &mut work.kind {
                            r.builder = None;
                        }
                    }
                }
                return;
            }
            // `FUN_1400fafb0`: the haulers no longer on it.
            WorkKind::Trade(t) => {
                let gone: Vec<usize> = t
                    .haulers
                    .keys()
                    .copied()
                    .filter(|&c| self.characters[c].stack.holds_task(id).is_none())
                    .collect();
                if let WorkKind::Trade(t) = &mut work.kind {
                    for c in gone {
                        t.haulers.remove(&c);
                    }
                }
                return;
            }
            // `FUN_1400f4c90`: the worker no longer holds the task.
            WorkKind::Location(l) => {
                if let Some(c) = l.worker {
                    if self.characters[c].stack.holds_task(id).is_none() {
                        if let WorkKind::Location(l) = &mut work.kind {
                            l.worker = None;
                        }
                    }
                }
                return;
            }
            // `FUN_1400f51f0`: the vendors no longer on it leave the plans.
            WorkKind::Market(m) => {
                let gone: Vec<usize> = m
                    .plans
                    .keys()
                    .copied()
                    .filter(|&c| self.characters[c].stack.holds_task(id).is_none())
                    .collect();
                if let WorkKind::Market(m) = &mut work.kind {
                    for c in gone {
                        m.plans.remove(&c);
                    }
                }
                return;
            }
            WorkKind::Test(_) => return,
        }
        .filter(|&c| self.characters[c].stack.holds_task(id).is_none())
        .collect();
        if gone.is_empty() {
            return;
        }
        match &mut work.kind {
            WorkKind::Gather(g) => {
                for c in &gone {
                    g.workers.remove(c);
                }
            }
            WorkKind::Build(b) => {
                for c in &gone {
                    b.workers.remove(c);
                }
                let held: Vec<i32> = b
                    .workers
                    .iter()
                    .filter_map(|&c| self.characters[c].stack.holds_task(id))
                    .collect();
                for (k, o) in b.occupied.iter_mut().enumerate() {
                    *o = held.contains(&(k as i32));
                }
            }
            WorkKind::Herbalist(h)
            | WorkKind::Forester(h)
            | WorkKind::Hunt(h)
            | WorkKind::TendArea(h)
            | WorkKind::Pasture(h)
            | WorkKind::ClearStorage(h)
            | WorkKind::Fire(h) => {
                for c in &gone {
                    h.workers.remove(c);
                }
            }
            WorkKind::Clear(_)
            | WorkKind::Pickup(_)
            | WorkKind::Produce(_)
            | WorkKind::Fishing(_)
            | WorkKind::PlantArea(_)
            | WorkKind::Harvest(_)
            | WorkKind::PlantOrchard(_)
            | WorkKind::PickFruit(_)
            | WorkKind::Location(_)
            | WorkKind::Market(_)
            | WorkKind::Trade(_)
            | WorkKind::Road(_)
            | WorkKind::Test(_) => {}
        }
        if let WorkKind::Gather(_) = work.kind {
            self.cancel(work);
            if let WorkKind::Gather(g) = &mut work.kind {
                g.transit.iter_mut().for_each(|t| *t = 0);
            }
        }
    }

    fn try_assign(&mut self, work: &mut Work, c: usize, time: i64) -> u8 {
        match work.kind {
            WorkKind::Gather(_) => self.gather_try(work, c, time),
            WorkKind::Build(_) => self.build_try(work, c),
            WorkKind::Clear(_) => self.clear_try(work, c, time),
            WorkKind::Pickup(_) => self.pickup_try(work, c, time),
            WorkKind::Produce(_) => self.produce_try(work, c, time),
            WorkKind::Location(_) => self.location_try(work, c),
            WorkKind::ClearStorage(_) => self.clear_storage_try(work, c, time),
            WorkKind::Fire(_) => self.fire_try(work, c, time),
            WorkKind::Trade(_) => self.trade_try(work, c),
            WorkKind::Road(_) => self.road_try(work, c, time),
            WorkKind::Market(_) => self.market_try(work, c),
            WorkKind::Fishing(_) => self.fishing_try(work, c),
            WorkKind::PlantArea(_) => self.plant_try(work, c),
            WorkKind::PlantOrchard(_) => self.plant_orchard_try(work, c),
            WorkKind::TendArea(_) => {
                if self
                    .sites
                    .get(&work.entity)
                    .is_some_and(|s| s.orchard.is_some())
                {
                    self.tend_orchard_try(work, c)
                } else {
                    self.tend_try(work, c)
                }
            }
            WorkKind::PickFruit(_) => {
                let works: BTreeSet<i32> = self.piles.values().map(|p| p.work).collect();
                if self.auto_pickup_offers(work.entity, |id| works.contains(&id)) {
                    return NESTED;
                }
                self.pick_fruit_try(work, c)
            }
            WorkKind::Harvest(_) => {
                let works: BTreeSet<i32> = self.piles.values().map(|p| p.work).collect();
                if self.auto_pickup_offers(work.entity, |id| works.contains(&id)) {
                    return NESTED;
                }
                self.harvest_try(work, c)
            }
            WorkKind::Herbalist(_)
            | WorkKind::Forester(_)
            | WorkKind::Hunt(_)
            | WorkKind::Pasture(_) => {
                self.pasture_stage = false;
                let works: BTreeSet<i32> = self.piles.values().map(|p| p.work).collect();
                if self.auto_pickup_offers(work.entity, |id| works.contains(&id)) {
                    return NESTED;
                }
                self.resume(work, c)
            }
            WorkKind::Test(_) => 0,
        }
    }

    fn take_offers(&mut self) -> Vec<i32> {
        std::mem::take(&mut self.offers)
    }

    fn offers_again(&mut self) -> bool {
        std::mem::take(&mut self.offers_again)
    }

    fn resume(&mut self, work: &mut Work, c: usize) -> u8 {
        match work.kind {
            WorkKind::Produce(_) => self.produce_resume(work, c),
            WorkKind::Herbalist(_) => self.herbalist_try(work, c),
            WorkKind::Forester(_) => self.forester_try(work, c),
            WorkKind::Hunt(_) => self.hunt_try(work, c),
            WorkKind::Harvest(_) => self.harvest_try(work, c),
            WorkKind::PickFruit(_) => self.pick_fruit_try(work, c),
            WorkKind::Pasture(_) => self.pasture_try(work, c),
            _ => 1,
        }
    }

    fn complete(&mut self, work: &mut Work, c: usize, param: i32) -> bool {
        match &mut work.kind {
            WorkKind::Gather(g) => {
                g.workers.remove(&c);
                let i = (param & 0xff) as usize;
                if let Some(t) = g.transit.get_mut(i) {
                    *t -= param >> 8;
                }
                if let Some(s) = g.stored.get_mut(i) {
                    *s += param >> 8;
                }
            }
            WorkKind::Build(b) => {
                b.workers.remove(&c);
                if let Some(o) = b.occupied.get_mut(param as usize) {
                    *o = false;
                }
            }
            // `FUN_1400eb300`: the resource (or cell) is done.
            WorkKind::Clear(cl) => {
                cl.resources.retain(|_, &mut w| w != c);
                cl.cells.remove(&param);
            }
            // `FUN_1400f8390`.
            WorkKind::Pickup(_) => {
                if let Some(p) = self.piles.get_mut(&work.entity) {
                    p.claimer = None;
                    p.state = 2;
                }
            }
            WorkKind::Produce(_) => Self::produce_complete(work, param, false),
            WorkKind::Fishing(_) => Self::fishing_complete(work, param),
            WorkKind::PlantArea(_) | WorkKind::Harvest(_) => Self::area_complete(work, param),
            WorkKind::PlantOrchard(_) => Self::plant_orchard_done(work, param),
            WorkKind::PickFruit(_) => Self::pick_done(work, param, false),
            // `FUN_1400f4d20`: free for the next one; the work stays.
            WorkKind::Location(l) => l.worker = None,
            // `FUN_1400f3860` / `FUN_1400f0370` / `FUN_1400f7820` / `FUN_1400ec100`.
            WorkKind::Herbalist(h)
            | WorkKind::Forester(h)
            | WorkKind::Hunt(h)
            | WorkKind::TendArea(h)
            | WorkKind::Pasture(h)
            | WorkKind::ClearStorage(h)
            // `FUN_1400eead0`.
            | WorkKind::Fire(h) => {
                h.workers.remove(&c);
            }
            // `FUN_1400f5430`.
            WorkKind::Market(m) => {
                m.plans.remove(&c);
            }
            // `FUN_1400fb1f0`.
            WorkKind::Trade(t) => {
                t.haulers.remove(&c);
            }
            // `FUN_1400e9620`: parameter 1 (the cell built) ends the work.
            WorkKind::Road(r) => {
                r.builder = None;
                return param == 1;
            }
            WorkKind::Test(_) => {}
        }
        false
    }

    fn fail(&mut self, work: &mut Work, c: usize, param: i32) {
        match &mut work.kind {
            WorkKind::Gather(g) => {
                g.workers.remove(&c);
                if let Some(t) = g.transit.get_mut((param & 0xff) as usize) {
                    *t -= param >> 8;
                }
            }
            WorkKind::Build(b) => {
                b.workers.remove(&c);
                if let Some(o) = b.occupied.get_mut(param as usize) {
                    *o = false;
                }
            }
            // `FUN_1400eb2d0` calls the complete slot.
            WorkKind::Clear(cl) => {
                cl.resources.retain(|_, &mut w| w != c);
                cl.cells.remove(&param);
            }
            // `FUN_1400f8350`.
            WorkKind::Pickup(_) => {
                if let Some(p) = self.piles.get_mut(&work.entity) {
                    p.claimer = None;
                    p.state = 0;
                }
            }
            WorkKind::Produce(_) => Self::produce_complete(work, param, true),
            // `FUN_1400ef350` calls the complete slot.
            WorkKind::Fishing(_) => Self::fishing_complete(work, param),
            WorkKind::PlantArea(_) | WorkKind::Harvest(_) => Self::area_fail(work, param),
            WorkKind::PlantOrchard(_) => Self::plant_orchard_done(work, param),
            WorkKind::PickFruit(_) => Self::pick_done(work, param, true),
            WorkKind::Location(l) => l.worker = None,
            WorkKind::Pasture(_) => self.pasture_fail(work, c, param),
            // `FUN_1400f3850` / `FUN_1400f0360` / `FUN_1400ec0f0` / `FUN_1400eeac0`.
            WorkKind::ClearStorage(h) | WorkKind::Fire(h) => {
                h.workers.remove(&c);
            }
            WorkKind::Herbalist(h)
            | WorkKind::Forester(h)
            | WorkKind::Hunt(h)
            | WorkKind::TendArea(h) => {
                h.workers.remove(&c);
            }
            // `FUN_1400f5420`.
            WorkKind::Market(m) => {
                m.plans.remove(&c);
            }
            // `FUN_1400fb1e0`.
            WorkKind::Trade(t) => {
                t.haulers.remove(&c);
            }
            // `FUN_1400e95f0`.
            WorkKind::Road(r) => r.builder = None,
            WorkKind::Test(_) => {}
        }
    }

    /// `FUN_1400f1950` / `FUN_1400eac70`: clear every worker's stack (their aborts leave the
    /// set through the fail slot).
    fn cancel(&mut self, work: &mut Work) {
        if let WorkKind::Produce(_) = work.kind {
            return self.produce_cancel(work);
        }
        loop {
            let next = match &work.kind {
                WorkKind::Gather(g) => g.workers.first().copied(),
                WorkKind::Build(b) => b.workers.first().copied(),
                // `FUN_1400ebb20`: every claimer of a resource or cell.
                WorkKind::Clear(cl) => cl
                    .resources
                    .values()
                    .chain(cl.cells.values())
                    .next()
                    .copied(),
                // `FUN_1400f8ec0`: the claimer, while claimed.
                WorkKind::Pickup(_) => self
                    .piles
                    .get(&work.entity)
                    .filter(|p| p.state == 1)
                    .and_then(|p| p.claimer),
                // `FUN_1400f4fc0`: the worker's stack is cleared.
                WorkKind::Location(l) => l.worker,
                // `FUN_1400efd00`: every worker's stack is cleared.
                WorkKind::Fishing(fw) => fw.workers.iter().flatten().next().copied(),
                // `FUN_1400f9930` / `FUN_1400f2740`.
                WorkKind::PlantArea(_)
                | WorkKind::Harvest(_)
                | WorkKind::PlantOrchard(_)
                | WorkKind::PickFruit(_) => Self::area_workers(work).first().copied(),
                // `FUN_1400f3fb0` / `FUN_1400f0d70` / `FUN_1400f8060`.
                WorkKind::Herbalist(h)
                | WorkKind::Forester(h)
                | WorkKind::Hunt(h)
                | WorkKind::TendArea(h)
                | WorkKind::Pasture(h)
                | WorkKind::ClearStorage(h)
                // `FUN_1400eee70`.
                | WorkKind::Fire(h) => h.workers.first().copied(),
                // `FUN_1400f7170`: every vendor's stack is cleared.
                WorkKind::Market(m) => m.plans.keys().next().copied(),
                // `FUN_1400fc0b0`.
                WorkKind::Trade(t) => t.haulers.keys().next().copied(),
                // `FUN_1400e9de0`.
                WorkKind::Road(r) => r.builder,
                WorkKind::Produce(_) | WorkKind::Test(_) => None,
            };
            let Some(c) = next else { return };
            self.clear_citizen(c, Some(work));
            // A worker without the task would loop forever in the original; leave it.
            match &mut work.kind {
                WorkKind::Gather(g) => {
                    g.workers.remove(&c);
                }
                WorkKind::Build(b) => {
                    b.workers.remove(&c);
                }
                WorkKind::Clear(cl) => {
                    cl.resources.retain(|_, &mut w| w != c);
                    cl.cells.retain(|_, &mut w| w != c);
                }
                WorkKind::Pickup(_) => {
                    if let Some(p) = self.piles.get_mut(&work.entity) {
                        p.claimer = None;
                        p.state = 0;
                    }
                }
                WorkKind::Location(l) => l.worker = None,
                WorkKind::Herbalist(h)
                | WorkKind::Forester(h)
                | WorkKind::Hunt(h)
                | WorkKind::TendArea(h)
                | WorkKind::Pasture(h)
                | WorkKind::ClearStorage(h)
                | WorkKind::Fire(h) => {
                    h.workers.remove(&c);
                }
                WorkKind::Fishing(fw) => {
                    for w in fw.workers.iter_mut().filter(|w| **w == Some(c)) {
                        *w = None;
                    }
                }
                WorkKind::PlantArea(a)
                | WorkKind::Harvest(a)
                | WorkKind::PlantOrchard(a)
                | WorkKind::PickFruit(a) => {
                    let cells: Vec<i32> = a
                        .claims
                        .iter()
                        .filter(|(_, &w)| w == c)
                        .map(|(&i, _)| i)
                        .collect();
                    for i in cells {
                        a.claims.remove(&i);
                        if let Some(n) = a.need.get_mut(i as usize) {
                            *n = true;
                        }
                    }
                }
                WorkKind::Market(m) => {
                    m.plans.remove(&c);
                }
                WorkKind::Trade(t) => {
                    t.haulers.remove(&c);
                }
                WorkKind::Road(r) => r.builder = None,
                WorkKind::Produce(_) | WorkKind::Test(_) => {}
            }
        }
    }
}
