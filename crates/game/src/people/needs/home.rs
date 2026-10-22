//! Needs met at home or from storage: eating, getting warm and stocking the home.

use super::*;

impl Needs<'_, '_> {
    /// Activities 0 and 14: eat, at home or from a storage.
    ///
    /// Original: `FUN_140112250`.
    pub fn eat(&mut self, who: &mut Who) -> bool {
        let cell = self.town.cell_of(who.ch.position);
        let home = who.state.home;
        let mut chosen = None;
        if self.life.f(0xcc) < who.state.hunger {
            // Urgent: food at home is eaten where the citizen stands.
            if let Some(h) = home {
                if 0 < self.home_food(h) {
                    chosen = Some((h, who.ch.position, self.warm_home(h)));
                }
            }
            if chosen.is_none() {
                if let Some((e, p)) = self.urgent_search(cell, home) {
                    let warm = self.warm_home(e);
                    chosen = Some((e, p, warm));
                }
            }
        }
        if chosen.is_none() {
            if let Some(h) = home {
                if self.town.can_reach(cell, h, self.rng) && 0 < self.home_food(h) {
                    chosen = Some((h, self.point(h), true));
                }
            }
        }
        if chosen.is_none() {
            match self.search(0, cell, home) {
                Some((e, p)) => {
                    let warm = self.warm_home(e);
                    chosen = Some((e, p, warm));
                }
                None => {
                    who.ai.cooldowns[0] = ticks(RETRY);
                    who.ai.cooldowns[14] = ticks(RETRY);
                    return false;
                }
            }
        }
        let (entity, target, warm) = chosen.unwrap();
        // `FUN_1400c28f0`.
        let facing = work_facing(self.centre(entity), target);
        let mut stack = self.begin(who);
        if warm {
            stack.push(who.ch, who.group, citizen_message(0, entity));
        }
        stack.push(who.ch, who.group, citizen_message(2, entity));
        stack.push(
            who.ch,
            who.group,
            Action::PerformWork {
                facing,
                anim: 8,
                duration: ticks(2.0),
                elapsed: 0,
                consume: false,
            },
        );
        let walk = MoveTo::new(target, 1, 0, WALKABLE, 0, -1.0, self.rng);
        stack.push(who.ch, who.group, Action::move_to(walk));
        who.ch.stack = stack;
        who.state.job = -1;
        who.state.health_events |= 4;
        who.state.supply_activity = 0;
        true
    }

    /// Activities 1 and 15: get warm, at the nearest warm residence when the citizen is very cold,
    /// else at a warm home.
    ///
    /// Original: `FUN_140112790`.
    pub fn warm(&mut self, who: &mut Who) -> bool {
        let cell = self.town.cell_of(who.ch.position);
        let mut chosen = None;
        if self.life.f(0xec) < who.state.cold {
            // `FUN_140094140`: path rank to each registered residence's point 0.
            let (size, terrain) = (self.size, self.terrain);
            let floors = self.town.floors;
            let ground_at = move |c: i32| crate::town::floor_ground(floors, size, terrain, c);
            let mut best = i32::MAX;
            let homes: Vec<usize> = self
                .residences
                .iter()
                .copied()
                .filter(|e| self.homes.contains_key(e))
                .collect();
            for e in homes {
                let p = self.point(e);
                let to = self.town.cell_of(p);
                let mut g = StorageGeometry {
                    town: self.town,
                    sectors: self.sectors,
                    ground: &ground_at,
                };
                let rank = crate::work::stock::StorageSearchMap::rank(&mut g, to, cell);
                if rank < best
                    && self.town.reachable(WALKABLE, cell, to)
                    && self.warm_home(e)
                    && rank >= 0
                {
                    best = rank;
                    chosen = Some((e, p));
                }
            }
        }
        if chosen.is_none() {
            if let Some(h) = who.state.home.filter(|&h| self.warm_home(h)) {
                chosen = Some((h, self.point(h)));
            }
        }
        let Some((entity, target)) = chosen else {
            who.ai.cooldowns[1] = ticks(RETRY);
            who.ai.cooldowns[15] = ticks(RETRY);
            return false;
        };
        // `FUN_1400c2c60`: no facing (−1000).
        let mut stack = self.begin(who);
        stack.push(who.ch, who.group, citizen_message(0, entity));
        stack.push(
            who.ch,
            who.group,
            Action::PerformWork {
                facing: -1000.0,
                anim: 8,
                duration: ticks(4.0),
                elapsed: 0,
                consume: false,
            },
        );
        let walk = MoveTo::new(target, 1, 0, WALKABLE, 0, -1.0, self.rng);
        stack.push(who.ch, who.group, Action::move_to(walk));
        who.ch.stack = stack;
        who.state.job = -1;
        who.state.health_events |= 4;
        who.state.supply_activity = 0;
        true
    }

    /// Activities 6 (food) and 7 (firewood, when `fuel`): carry food or firewood home from the
    /// nearest storage. The caller first drops off anything carried (and queues this activity
    /// again). `reachable` is the caller's test that the home can be reached.
    ///
    /// Original: `FUN_140111160` (food), `FUN_1401108b0` (firewood); the reach test is
    /// `FUN_14011ada0`.
    pub fn stock_home(&mut self, who: &mut Who, fuel: bool, reachable: bool) -> bool {
        let (slot, category, icon, supply) = if fuel { (7, 1, 7, 2) } else { (6, 0, 6, 1) };
        let cell = self.town.cell_of(who.ch.position);
        let Some(home) = who.state.home.filter(|_| reachable) else {
            who.ai.cooldowns[slot] = ticks(self.life.f(0xd8) * STOCK_UNREACHABLE);
            return false;
        };
        let Some(home_storage) = self.storage(home) else {
            return false;
        };
        let Some((source, from)) = self.search(category, cell, Some(home)) else {
            who.ai.cooldowns[slot] = ticks(self.life.f(0xd8) * STOCK_NOTHING);
            who.state.flags |= 4;
            if stored(
                &self.storages[home_storage],
                self.materials,
                if fuel { 6 } else { 5 },
            ) == 0
            {
                if let Some(h) = self.homes.get_mut(&home) {
                    crate::people::status::set_icon(&mut h.status, icon, true, false);
                }
            }
            return false;
        };
        if let Some(h) = self.homes.get_mut(&home) {
            crate::people::status::set_icon(&mut h.status, icon, false, false);
        }
        let Some(source_storage) = self.storage(source) else {
            return false;
        };
        let target = self.point(home);
        // As many units as the larger of the laborer's carry and the free carried capacity.
        let free = who.state.carried.capacity - who.state.carried.used(self.materials);
        let mut room = self.carry.max(free);
        let mut items: Vec<(usize, i32)> = Vec::new();
        while let Some(m) =
            self.storages[source_storage].find_item(self.materials, category, self.rng)
        {
            let size = self.materials[m].size;
            if room < size {
                break;
            }
            self.storages[source_storage].reserve(m, 1);
            match items.iter_mut().find(|(x, _)| *x == m) {
                Some((_, n)) => *n += 1,
                None => items.push((m, 1)),
            }
            room -= size;
        }
        // `FUN_1400c4c60` with no work: pick up, carry, drop everything, warm up at home.
        let face_target = work_facing(self.centre(home), target);
        let face_source = work_facing(self.centre(source), from);
        let perform = |facing: f32, anim: usize| Action::PerformWork {
            facing,
            anim,
            duration: ticks(0.75),
            elapsed: 0,
            consume: false,
        };
        let mut stack = self.begin(who);
        stack.push(who.ch, who.group, citizen_message(0, home));
        stack.push(who.ch, who.group, perform(face_target, 7));
        stack.push(who.ch, who.group, Action::ShowTool(-1));
        stack.push(
            who.ch,
            who.group,
            Action::DropOff {
                storage: home_storage,
                material: usize::MAX,
                count: -1,
                // `FUN_1400c4c60`'s drop-off redirects.
                redirect: true,
            },
        );
        stack.push(who.ch, who.group, perform(face_target, 6));
        let walk = MoveTo::new(target, 4, 0, WALKABLE, 0, -1.0, self.rng);
        stack.push(who.ch, who.group, Action::move_to(walk));
        stack.push(who.ch, who.group, perform(face_source, 7));
        stack.push(who.ch, who.group, Action::ShowTool(-2));
        stack.push(
            who.ch,
            who.group,
            Action::PickupStoredRawMaterial {
                storage: source_storage,
                items,
                partial: true,
            },
        );
        stack.push(who.ch, who.group, perform(face_source, 6));
        let walk = MoveTo::new(from, 1, 0, WALKABLE, 0, -1.0, self.rng);
        stack.push(who.ch, who.group, Action::move_to(walk));
        who.ch.stack = stack;
        who.state.health_events |= 4;
        who.state.job = -1;
        who.state.supply_activity = supply;
        true
    }
}
