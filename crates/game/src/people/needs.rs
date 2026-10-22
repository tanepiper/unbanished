//! The citizen activities that meet needs: eating (activities 0 and 14), warming up (1 and 15) and
//! stocking the home with food and firewood (6 and 7). See docs/CITIZEN-LIFE.md.
//!
//! Original: `FUN_140112250`, `FUN_140112790`, `FUN_140111160`, `FUN_1401108b0`.

mod home;
mod services;

use std::collections::BTreeMap;

use unbanished_formats::types::AnimationGroup;

use crate::behaviour::activities::{StorageGeometry, TownView};
use crate::behaviour::agents::CitizenState;
use crate::behaviour::ai::CitizenAi;
use crate::behaviour::character::{work_facing, Action, Character, Component, Message, MoveTo};
use crate::pathfind::WALKABLE;
use crate::people::life::{stored, Home, LifeRules};
use crate::random::Random;
use crate::scheduler::ticks;
use crate::sectors::SectorGraph;
use crate::terrain::HeightGrid;
use crate::work::stock::{find_storage, RawMaterial, Storage, StorageSite};

/// `DAT_140178890`: an activity that found nothing waits this long (both its slots).
const RETRY: f32 = 3.75;
/// `DAT_1401788fc`, `DAT_140178934`: home-stocking cooldowns are `D[0xd8]` times these.
const STOCK_UNREACHABLE: f32 = 75.0;
const STOCK_NOTHING: f32 = 300.0;

/// What the activities read and change besides the citizen.
pub struct Needs<'a, 'b> {
    /// A view of the town.
    pub town: &'a TownView<'b>,
    /// The sector graph.
    pub sectors: &'a mut SectorGraph,
    /// The storage sites.
    pub sites: &'a [StorageSite],
    /// The town's storages.
    pub storages: &'a mut [Storage],
    /// The raw materials.
    pub materials: &'a [RawMaterial],
    /// The homes, by entity.
    pub homes: &'a mut BTreeMap<usize, Home>,
    // +0x20
    /// The town's residences, in the order they were completed: the searches over residences keep
    /// the first of equal ranks.
    ///
    /// Original: `FUN_140093840`, `FUN_140094140`.
    pub residences: &'a [usize],
    /// The environment's temperature.
    pub temperature: f32,
    /// The citizen description's rules.
    pub life: &'a LifeRules,
    // +0x50
    /// The laborer profession's carry.
    pub carry: i32,
    /// The ground's heights.
    pub terrain: &'a HeightGrid<'b>,
    /// The map's size in cells.
    pub size: usize,
    /// The game's random numbers.
    pub rng: &'a mut Random,
    // +0x70
    /// The town's HealerComponents.
    pub healers: &'a [usize],
    /// Every WorkPlaceComponent, by entity.
    pub workplaces: &'a BTreeMap<usize, crate::work::professions::WorkPlace>,
    /// The AttendanceComponents, by entity.
    pub attendance: &'a mut BTreeMap<usize, crate::work::construction::Attendance>,
    /// Every citizen (play follows the guardian).
    pub citizens: &'a [crate::people::citizens::Citizen],
    /// Set when the school activity found no place to sit: the caller leaves the school and
    /// looks for another.
    ///
    /// Original: `FUN_14011af20(c, 1)`.
    pub reschool: bool,
}

/// The citizen an activity runs for.
pub struct Who<'a> {
    /// The citizen's AI.
    pub ai: &'a mut CitizenAi,
    /// The citizen's character.
    pub ch: &'a mut Character,
    /// The citizen's state.
    pub state: &'a mut CitizenState,
    /// The citizen's animation group.
    pub group: &'a AnimationGroup,
}

impl Needs<'_, '_> {
    /// `Interact::PointPosition(entity, 0)`: the first use point, else the rectangle centre.
    fn point(&self, entity: usize) -> [f32; 3] {
        let e = &self.town.entities[entity];
        if let Some(p) = e.use_points.first() {
            return *p;
        }
        crate::town::floor_ground(
            self.town.floors,
            self.size,
            self.terrain,
            self.town.point_cell(entity),
        )
    }

    /// `FUN_1400c7e70`'s source: the model position, else the rectangle centre.
    fn centre(&self, entity: usize) -> [f32; 3] {
        let e = &self.town.entities[entity];
        if e.has_model {
            e.position
        } else {
            crate::town::floor_ground(
                self.town.floors,
                self.size,
                self.terrain,
                self.town.point_cell(entity),
            )
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
        let floors = self.town.floors;
        let ground = move |c: i32| crate::town::floor_ground(floors, size, terrain, c);
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

    /// `FUN_140093840`: for the urgently hungry, the nearest of the home, the town's markets
    /// and storages, and the empty residences that hold food, each taking a strictly better
    /// rank in that order.
    /// - The home: its entrance (`FUN_14008d480`, the ground at the cell's centre), when
    ///   reachable, at its path rank (`FUN_140008830`), if it holds food (`FUN_140094410`).
    /// - The markets, then the storages, as `FUN_140093200` weighs them but without its stop
    ///   at a market covering the home.
    /// - Each residence nobody lives in (`FUN_140017ac0`): its point 0, whose path rank
    ///   (`FUN_1400a4ce0`, else the map width plus the straight distance) must beat the best;
    ///   then the cells must be connected and the residence must hold food, and that rank is
    ///   taken.
    fn urgent_search(&mut self, cell: i32, home: Option<usize>) -> Option<(usize, [f32; 3])> {
        let mut best = i32::MAX;
        let mut found = None;
        let (size, terrain) = (self.size, self.terrain);
        let floors = self.town.floors;
        let ground_at = move |c: i32| crate::town::floor_ground(floors, size, terrain, c);
        if let Some(h) = home {
            let entrance = self.town.entrance(h, self.rng);
            if entrance >= 0 && self.town.reachable(WALKABLE, cell, entrance) {
                let rank = self.rank(entrance, cell);
                let food = self
                    .homes
                    .get(&h)
                    .and_then(|r| self.storages[r.storage].find_item(self.materials, 0, self.rng));
                if food.is_some() {
                    best = rank;
                    found = Some((h, ground_at(entrance)));
                }
            }
        }
        let (rank, storage) = {
            let mut geometry = StorageGeometry {
                town: self.town,
                sectors: self.sectors,
                ground: &ground_at,
            };
            crate::work::stock::scan_storages(
                self.sites,
                self.storages,
                self.materials,
                0,
                cell,
                None,
                best,
                &mut geometry,
                self.rng,
            )
        };
        if let Some(m) = storage {
            best = rank;
            found = Some((self.sites[m.site].entity, m.target));
        }
        let residences: Vec<usize> = self.residences.to_vec();
        for e in residences {
            let Some(home) = self.homes.get(&e) else {
                continue;
            };
            if home.occupants() > 0 {
                continue;
            }
            let p = self.point(e);
            let to = self.town.cell_of(p);
            let rank = self.rank(to, cell);
            if best <= rank || !self.town.reachable(WALKABLE, cell, to) {
                continue;
            }
            let storage = self.homes[&e].storage;
            if self.storages[storage]
                .find_item(self.materials, 0, self.rng)
                .is_some()
                && rank >= 0
            {
                best = rank;
                found = Some((e, p));
            }
        }
        found
    }

    /// The action stack emptied and set to call the AI back when done.
    fn begin(&mut self, who: &mut Who) -> crate::behaviour::character::ActionStack {
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
}

impl Needs<'_, '_> {
    /// The path-rank distance from `a` to `b`.
    fn rank(&mut self, a: i32, b: i32) -> i32 {
        let (size, terrain) = (self.size, self.terrain);
        let floors = self.town.floors;
        let ground_at = move |c: i32| crate::town::floor_ground(floors, size, terrain, c);
        let mut g = StorageGeometry {
            town: self.town,
            sectors: self.sectors,
            ground: &ground_at,
        };
        crate::work::stock::StorageSearchMap::rank(&mut g, a, b)
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

/// The home holds food for every living family (`D[0xc4]` per member of this citizen's family).
///
/// Original: `FUN_14011abb0`.
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

/// `D[0xd4]` firewood per living family.
///
/// Original: `FUN_14011acc0`.
pub fn home_has_fuel(
    home: &Home,
    storages: &[Storage],
    materials: &[RawMaterial],
    life: &LifeRules,
) -> bool {
    home.families_living() as f32 * life.f(0xd4)
        <= stored(&storages[home.storage], materials, 6) as f32
}
