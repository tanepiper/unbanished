//! The citizen activities that meet needs: eating (`FUN_140112250`, activities 0 and 14),
//! warming up (`FUN_140112790`, 1 and 15) and stocking the home with food and firewood
//! (`FUN_140111160` and `FUN_1401108b0`, 6 and 7). See docs/CITIZEN-LIFE.md.

use std::collections::BTreeMap;

use banished_formats::types::AnimationGroup;

use crate::activities::{StorageGeometry, TownView};
use crate::agents::CitizenState;
use crate::ai::CitizenAi;
use crate::character::{work_facing, Action, Character, Component, Message, MoveTo};
use crate::life::{stored, Home, LifeRules};
use crate::pathfind::WALKABLE;
use crate::random::Random;
use crate::scheduler::ticks;
use crate::sectors::SectorGraph;
use crate::stock::{find_storage, RawMaterial, Storage, StorageSite};
use crate::terrain::HeightGrid;

/// `DAT_140178890`: an activity that found nothing waits this long (both its slots).
const RETRY: f32 = 3.75;
/// `DAT_1401788fc`, `DAT_140178934`: home-stocking cooldowns are `D[0xd8]` times these.
const STOCK_UNREACHABLE: f32 = 75.0;
const STOCK_NOTHING: f32 = 300.0;

/// What the activities read and change besides the citizen.
pub struct Needs<'a, 'b> {
    pub town: &'a TownView<'b>,
    pub sectors: &'a mut SectorGraph,
    pub sites: &'a [StorageSite],
    pub storages: &'a mut [Storage],
    pub materials: &'a [RawMaterial],
    pub homes: &'a mut BTreeMap<usize, Home>,
    pub temperature: f32,
    pub life: &'a LifeRules,
    /// The laborer profession's carry `+0x50`.
    pub carry: i32,
    pub terrain: &'a HeightGrid<'b>,
    pub size: usize,
    pub rng: &'a mut Random,
    /// The town manager's HealerComponents (`+0x70`) and every WorkPlaceComponent.
    pub healers: &'a [usize],
    pub workplaces: &'a BTreeMap<usize, crate::professions::WorkPlace>,
    pub attendance: &'a mut BTreeMap<usize, crate::construction::Attendance>,
    /// Every citizen (play follows the guardian).
    pub citizens: &'a [crate::citizens::Citizen],
    /// Set when the school activity found no place to sit: the caller leaves the school and
    /// looks for another (`FUN_14011af20(c, 1)`).
    pub reschool: bool,
}

/// The citizen an activity runs for.
pub struct Who<'a> {
    pub ai: &'a mut CitizenAi,
    pub ch: &'a mut Character,
    pub state: &'a mut CitizenState,
    pub group: &'a AnimationGroup,
}

fn ground(size: usize, terrain: &HeightGrid, cell: i32) -> [f32; 3] {
    let n = size as i32;
    let h = (n >> 1) as f32 - 0.5;
    let (x, y) = ((cell % n) as f32 - h, (cell / n) as f32 - h);
    let mut normal = [0.0; 3];
    [x, y, terrain.ground(x, y, &mut normal)]
}

impl Needs<'_, '_> {
    /// `Interact::PointPosition(entity, 0)`: the first use point, else the rectangle centre.
    fn point(&self, entity: usize) -> [f32; 3] {
        let e = &self.town.entities[entity];
        if let Some(p) = e.use_points.first() {
            return *p;
        }
        ground(self.size, self.terrain, self.town.point_cell(entity))
    }

    /// `FUN_1400c7e70`'s source: the model position, else the rectangle centre.
    fn centre(&self, entity: usize) -> [f32; 3] {
        let e = &self.town.entities[entity];
        if e.has_model {
            e.position
        } else {
            ground(self.size, self.terrain, self.town.point_cell(entity))
        }
    }

    fn home_food(&self, home: usize) -> i32 {
        self.homes
            .get(&home)
            .map_or(0, |h| stored(&self.storages[h.storage], self.materials, 5))
    }

    fn warm_home(&self, entity: usize) -> bool {
        self.homes
            .get(&entity)
            .is_some_and(|h| h.warm(self.temperature))
    }

    /// The storage of `entity` (a home's, or a registered town storage's).
    fn storage(&self, entity: usize) -> Option<usize> {
        self.homes.get(&entity).map(|h| h.storage).or_else(|| {
            self.sites
                .iter()
                .find(|s| s.entity == entity)
                .map(|s| s.storage)
        })
    }

    /// `FUN_140093200` over the town's storages for category `category` (0 food, 1 fuel).
    fn search(
        &mut self,
        category: u32,
        cell: i32,
        home: Option<usize>,
    ) -> Option<(usize, [f32; 3])> {
        let home_cell = home.map_or(-1, |h| self.town.entrance(h, self.rng));
        let (size, terrain) = (self.size, self.terrain);
        let ground = move |c: i32| ground(size, terrain, c);
        let mut geometry = StorageGeometry {
            town: self.town,
            sectors: self.sectors,
            ground: &ground,
        };
        let found = find_storage(
            self.sites,
            self.storages,
            self.materials,
            category,
            cell,
            home_cell,
            &mut geometry,
            self.rng,
        )?;
        Some((self.sites[found.site].entity, found.target))
    }

    /// `FUN_140093840`: for the urgently hungry, the nearest of the home, the town's storages
    /// and the empty residences that hold food.
    /// Deviation: the town's storages are ranked by `FUN_140093200` (with its market
    /// precedence) rather than this function's own scan.
    fn urgent_search(&mut self, cell: i32, home: Option<usize>) -> Option<(usize, [f32; 3])> {
        let mut best = i32::MAX;
        let mut found = None;
        let (size, terrain) = (self.size, self.terrain);
        let ground_at = move |c: i32| ground(size, terrain, c);
        if let Some(h) = home {
            let entrance = self.town.entrance(h, self.rng);
            if entrance >= 0 && self.town.reachable(WALKABLE, cell, entrance) {
                let rank = {
                    let mut g = StorageGeometry {
                        town: self.town,
                        sectors: self.sectors,
                        ground: &ground_at,
                    };
                    crate::stock::StorageSearchMap::rank(&mut g, entrance, cell)
                };
                if 0 < self.home_food(h) {
                    best = rank;
                    found = Some((h, self.point(h)));
                }
            }
        }
        if let Some((e, target)) = self.search(0, cell, home) {
            let to = self.town.cell_of(target);
            let mut g = StorageGeometry {
                town: self.town,
                sectors: self.sectors,
                ground: &ground_at,
            };
            let rank = crate::stock::StorageSearchMap::rank(&mut g, to, cell);
            if rank < best {
                best = rank;
                found = Some((e, target));
            }
        }
        let residences: Vec<usize> = self.homes.keys().copied().collect();
        for e in residences {
            if self.homes[&e].occupants() > 0 || self.home_food(e) < 1 {
                continue;
            }
            let p = self.point(e);
            let to = self.town.cell_of(p);
            if !self.town.reachable(WALKABLE, cell, to) {
                continue;
            }
            let mut g = StorageGeometry {
                town: self.town,
                sectors: self.sectors,
                ground: &ground_at,
            };
            let rank = crate::stock::StorageSearchMap::rank(&mut g, to, cell);
            if rank >= 0 && rank < best {
                best = rank;
                found = Some((e, p));
            }
        }
        found
    }

    /// The action stack emptied and set to call the AI back when done.
    fn begin(&mut self, who: &mut Who) -> crate::character::ActionStack {
        let mut stack = std::mem::take(&mut who.ch.stack);
        stack.clear(who.ch, who.group, self.storages);
        stack.set_on_empty(vec![Action::SendMessage(Message {
            component: Component::CitizenAi,
            message: 0,
            entity: None,
            a: 0,
            b: 0,
        })]);
        stack
    }

    /// `FUN_140112250` (activities 0 and 14).
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

    /// `FUN_140112790` (activities 1 and 15): the nearest warm residence when urgently cold,
    /// else a warm home.
    pub fn warm(&mut self, who: &mut Who) -> bool {
        let cell = self.town.cell_of(who.ch.position);
        let mut chosen = None;
        if self.life.f(0xec) < who.state.cold {
            // `FUN_140094140`: path rank to each registered residence's point 0.
            let (size, terrain) = (self.size, self.terrain);
            let ground_at = move |c: i32| ground(size, terrain, c);
            let mut best = i32::MAX;
            let homes: Vec<usize> = self.homes.keys().copied().collect();
            for e in homes {
                let p = self.point(e);
                let to = self.town.cell_of(p);
                let mut g = StorageGeometry {
                    town: self.town,
                    sectors: self.sectors,
                    ground: &ground_at,
                };
                let rank = crate::stock::StorageSearchMap::rank(&mut g, to, cell);
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

    /// `FUN_140111160` (`fuel` false, activity 6) and `FUN_1401108b0` (`fuel`, activity 7):
    /// carry food or firewood home from the nearest storage. The caller first drops off
    /// anything carried (and queues this activity again).
    /// `reachable` is the caller's `FUN_14011ada0` result for the home.
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
                    crate::status::set_icon(&mut h.status, icon, true, false);
                }
            }
            return false;
        };
        if let Some(h) = self.homes.get_mut(&home) {
            crate::status::set_icon(&mut h.status, icon, false, false);
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

impl Needs<'_, '_> {
    /// The path-rank distance from `a` to `b`.
    fn rank(&mut self, a: i32, b: i32) -> i32 {
        let (size, terrain) = (self.size, self.terrain);
        let ground_at = move |c: i32| ground(size, terrain, c);
        let mut g = StorageGeometry {
            town: self.town,
            sectors: self.sectors,
            ground: &ground_at,
        };
        crate::stock::StorageSearchMap::rank(&mut g, a, b)
    }

    /// `FUN_1401280b0` over an entity's use points: the nearest reachable from `cell`.
    fn nearest_point(&mut self, e: usize, cell: i32) -> Option<[f32; 3]> {
        let points = self.town.entities.get(e)?.use_points.clone();
        let mut best = 0xffff;
        let mut found = None;
        for p in points {
            let to = self.town.cell_of(p);
            if !self.town.reachable(WALKABLE, cell, to) {
                continue;
            }
            let rank = self.rank(cell, to);
            if rank < best {
                best = rank;
                found = Some(p);
            }
        }
        found
    }

    /// `FUN_1400c0960(…, message, seconds, true)`: walk to `target`, work (no facing), then
    /// Citizen message `message`.
    fn visit(&mut self, who: &mut Who, target: [f32; 3], message: i32, secs: f32) {
        let mut stack = self.begin(who);
        stack.push(
            who.ch,
            who.group,
            Action::SendMessage(Message {
                component: Component::Citizen,
                message,
                entity: None,
                a: 0,
                b: 0,
            }),
        );
        stack.push(
            who.ch,
            who.group,
            Action::PerformWork {
                facing: -1000.0,
                anim: 8,
                duration: ticks(secs),
                elapsed: 0,
                consume: false,
            },
        );
        let walk = MoveTo::new(target, 1, 0, WALKABLE, 0, -1.0, self.rng);
        stack.push(who.ch, who.group, Action::move_to(walk));
        who.ch.stack = stack;
        who.state.health_events |= 4;
        who.state.job = -1;
        who.state.supply_activity = 0;
    }

    /// `FUN_1400c2f20`: rest at home (walk there, warm up, 5 s).
    fn rest_at_home(&mut self, who: &mut Who, home: usize) {
        let target = self.point(home);
        let mut stack = self.begin(who);
        stack.push(
            who.ch,
            who.group,
            Action::PerformWork {
                facing: -1000.0,
                anim: 8,
                duration: ticks(5.0),
                elapsed: 0,
                consume: false,
            },
        );
        stack.push(who.ch, who.group, citizen_message(0, home));
        let walk = MoveTo::new(target, 1, 0, WALKABLE, 0, -1.0, self.rng);
        stack.push(who.ch, who.group, Action::move_to(walk));
        who.ch.stack = stack;
        who.state.job = -1;
        who.state.health_events |= 4;
        who.state.supply_activity = 0;
    }

    /// `FUN_140112a30` (activity 10): sit a lesson at the school (`D[0x120]` s, message 4).
    pub fn school(&mut self, who: &mut Who) -> bool {
        let Some(school) = who.state.school else {
            return false;
        };
        let cell = self.town.cell_of(who.ch.position);
        match self.nearest_point(school, cell) {
            Some(target) => {
                self.visit(who, target, 4, self.life.f(0x120));
                true
            }
            None => {
                self.reschool = true;
                false
            }
        }
    }

    /// `FUN_140112c60` (activity 5): with a doctor, be treated (`D[0x124]` s, message 5);
    /// without one, or when the doctor cannot be reached, rest at a reachable home.
    /// Like the original, it reports "not started" after a treatment visit.
    pub fn doctor(&mut self, who: &mut Who, me: usize) -> bool {
        let cell = self.town.cell_of(who.ch.position);
        let home = who
            .state
            .home
            .filter(|&h| self.town.can_reach(cell, h, self.rng));
        let Some(doctor) = who.state.doctor else {
            return match home {
                Some(h) => {
                    self.rest_at_home(who, h);
                    true
                }
                None => false,
            };
        };
        match self.nearest_point(doctor, cell) {
            Some(target) => {
                self.visit(who, target, 5, self.life.f(0x124));
                false
            }
            None => {
                // `FUN_14011ae80`, then home.
                who.state.doctor = None;
                if let Some(a) = self.attendance.get_mut(&doctor) {
                    a.attendees.retain(|&x| x != me);
                }
                match who.state.home {
                    Some(h) => {
                        self.rest_at_home(who, h);
                        true
                    }
                    None => false,
                }
            }
        }
    }

    /// `FUN_140111d60` (activity 4): take a herb from the nearest storage to the nearest
    /// staffed herbalist, be treated there (5 s, message 3). The caller drops off first.
    pub fn herbs(&mut self, who: &mut Who) -> bool {
        who.ai.cooldowns[4] = ticks(75.0);
        let cell = self.town.cell_of(who.ch.position);
        let Some((source, from)) = self.search(3, cell, who.state.home) else {
            return false;
        };
        let Some(storage) = self.storage(source) else {
            return false;
        };
        let Some(herb) = self.storages[storage].find_item(self.materials, 3, self.rng) else {
            return false;
        };
        // `FUN_140094700`: the nearest staffed, reachable healer.
        let mut best = 0xffff;
        let mut healer = None;
        for &e in self.healers {
            if self.workplaces.get(&e).is_none_or(|w| w.workers.is_empty()) {
                continue;
            }
            let p = self.point(e);
            let to = self.town.cell_of(p);
            if !self.town.reachable(WALKABLE, cell, to) {
                continue;
            }
            let rank = self.rank(cell, to);
            if rank < best {
                best = rank;
                healer = Some((e, p));
            }
        }
        let Some((healer, at)) = healer else {
            return false;
        };
        // `FUN_1400c1c60`.
        let face_healer = work_facing(self.centre(healer), at);
        let face_source = work_facing(self.centre(source), from);
        self.storages[storage].reserve(herb, 1);
        let mut stack = self.begin(who);
        stack.push(who.ch, who.group, citizen_message(3, healer));
        stack.push(
            who.ch,
            who.group,
            Action::ConsumeStoredRawMaterial {
                storage: usize::MAX,
                items: vec![(herb, 1)],
                reserved: false,
            },
        );
        stack.push(
            who.ch,
            who.group,
            Action::PerformWork {
                facing: face_healer,
                anim: 8,
                duration: ticks(5.0),
                elapsed: 0,
                consume: false,
            },
        );
        let walk = MoveTo::new(at, 1, 0, WALKABLE, 0, -1.0, self.rng);
        stack.push(who.ch, who.group, Action::move_to(walk));
        stack.push(
            who.ch,
            who.group,
            Action::PickupStoredRawMaterial {
                storage,
                items: vec![(herb, 1)],
                partial: true,
            },
        );
        stack.push(
            who.ch,
            who.group,
            Action::PerformWork {
                facing: face_source,
                anim: 8,
                duration: ticks(1.0),
                elapsed: 0,
                consume: false,
            },
        );
        let walk = MoveTo::new(from, 1, 0, WALKABLE, 0, -1.0, self.rng);
        stack.push(who.ch, who.group, Action::move_to(walk));
        who.ch.stack = stack;
        who.state.health_events |= 4;
        who.state.job = -1;
        who.state.supply_activity = 0;
        who.ai.cooldowns[4] = ticks(self.life.f(0x78) * 75.0);
        true
    }

    /// `FUN_14010f780` (activity 11): a child plays next to its guardian (or the guardian's
    /// partner), then idles `RandFloat · 7 + 3` s.
    pub fn play(&mut self, who: &mut Who, me: usize) -> bool {
        let Some(mut g) = self.citizens.get(me).and_then(|c| c.parent) else {
            return false;
        };
        if let Some(p) = self.citizens[g].spouse {
            // `Rand(1)` is always 0: the partner.
            if self.rng.rand_below(1) == 0 {
                g = p;
            }
        }
        let from = self.town.cell_of(self.citizens[g].position);
        let to = self.town.cell_of(who.ch.position);
        let Some(cell) =
            crate::pathfind::nearest_neighbour(self.size, self.town.types, from, to, WALKABLE)
        else {
            return false;
        };
        let target = ground(self.size, self.terrain, cell);
        let idle = self.rng.rand_float() * 7.0 + 3.0;
        // `FUN_1400c7b90`.
        let mut stack = self.begin(who);
        if 0.0 < idle {
            stack.push(
                who.ch,
                who.group,
                Action::PlayAnimation {
                    anim: 0,
                    duration: ticks(idle),
                    elapsed: 0,
                },
            );
        }
        let walk = MoveTo::new(target, 1, 0, WALKABLE, 0, -1.0, self.rng);
        stack.push(who.ch, who.group, Action::move_to(walk));
        who.ch.stack = stack;
        true
    }
}

fn citizen_message(message: i32, entity: usize) -> Action {
    Action::SendMessage(Message {
        component: Component::Citizen,
        message,
        entity: Some(entity),
        a: 0,
        b: 0,
    })
}

/// `FUN_14011abb0`: the home holds food for every living family (`D[0xc4]` per member of
/// this citizen's family).
pub fn home_has_food(
    home: &Home,
    slot: i32,
    storages: &[Storage],
    materials: &[RawMaterial],
    life: &LifeRules,
) -> bool {
    let members = home.families.get(slot.max(0) as usize).map_or(0, Vec::len) as i32;
    home.families_living() * members * life.f(0xc4) as i32
        <= stored(&storages[home.storage], materials, 5)
}

/// `FUN_14011acc0`: `D[0xd4]` firewood per living family.
pub fn home_has_fuel(
    home: &Home,
    storages: &[Storage],
    materials: &[RawMaterial],
    life: &LifeRules,
) -> bool {
    home.families_living() as f32 * life.f(0xd4)
        <= stored(&storages[home.storage], materials, 6) as f32
}
