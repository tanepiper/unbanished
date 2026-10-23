//! Needs met elsewhere: school, the doctor, herbs and play.

use super::*;

impl Needs<'_, '_> {
    /// Activity 10: sit a lesson at the school (message 4; its length is a value of the citizen
    /// description).
    ///
    /// Original: `FUN_140112a30`.
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

    /// Activity 5: with a doctor, be treated (message 5); without one, or when the doctor cannot
    /// be reached, rest at a home that can be reached. A homeless citizen whose doctor cannot be
    /// reached still makes the visit, to the centre of the map. Like the original, it reports
    /// "not started" after a treatment visit.
    ///
    /// Original: `FUN_140112c60`.
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
        let target = match self.nearest_point(doctor, cell) {
            Some(target) => target,
            None => {
                // `FUN_14011ae80`, then home.
                who.state.doctor = None;
                if let Some(a) = self.attendance.get_mut(&doctor) {
                    a.attendees.retain(|&x| x != me);
                }
                if let Some(h) = who.state.home {
                    self.rest_at_home(who, h);
                    return true;
                }
                // Without a home the original falls through to the visit with the point search's
                // untouched output, the zeroed vector: the citizen walks to the map's centre.
                [0.0; 3]
            }
        };
        self.visit(who, target, 5, self.life.f(0x124));
        false
    }

    /// Activity 4: take herbs from the nearest storage to the nearest herbalist with a worker, and
    /// be treated there (5 seconds, message 3). The caller drops off what it carries first.
    ///
    /// Original: `FUN_140111d60`.
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

    /// Activity 11: a child plays next to its guardian (or the guardian's partner), then waits
    /// `RandFloat · 7 + 3` seconds.
    ///
    /// Original: `FUN_14010f780`.
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
        let target = crate::town::floor_ground(self.town.floors, self.size, self.terrain, cell);
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
