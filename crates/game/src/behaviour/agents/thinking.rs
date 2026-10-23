//! The citizens' AI: delivering messages, thinking and running the AI update.

mod delivery;

use super::*;

/// `FUN_1401146c0`'s targets that are ported; the rest report "nothing to do".
#[allow(clippy::too_many_arguments)]
pub(super) fn run(
    ai: &mut CitizenAi,
    a: u32,
    ch: &mut Character,
    state: &mut CitizenState,
    town: &TownView,
    sectors: &mut SectorGraph,
    stock: &mut TownStock,
    terrain: &HeightGrid,
    group: &AnimationGroup,
    rules: CitizenRulesAi,
    size: usize,
    rng: &mut Random,
) -> bool {
    match a {
        activity::TOOL | activity::CLOTHING => {
            // FUN_140111a50 / FUN_140111830: cooldown is set even when already equipped.
            ai.cooldowns[a as usize] = ticks(FETCH_COOLDOWN);
            let tool = a == activity::TOOL;
            if if tool {
                state.tool_uses > 0
            } else {
                state.clothing != 0
            } {
                return false;
            }
            let cell = town.cell_of(ch.position);
            let home = state.home.map_or(-1, |h| town.entrance(h, rng));
            let ground = |c| crate::town::floor_ground(town.floors, size, terrain, c);
            let mut geometry = StorageGeometry {
                town,
                sectors,
                ground: &ground,
            };
            let Some(found) = find_storage(
                stock.sites,
                stock.storages,
                stock.materials,
                if tool { 2 } else { 4 },
                cell,
                home,
                &mut geometry,
                rng,
            ) else {
                if tool {
                    state.tool_status = 0;
                    crate::people::status::set_icon(&mut state.status, 4, true, false);
                } else {
                    state.clothing_status = 0;
                }
                state.flags |= 4;
                // The app draws the StatusIconComponent's billboard from the mask.
                return false;
            };
            // FUN_1400c2540 / FUN_1400c2180: push in reverse execution order.
            let mut stack = std::mem::take(&mut ch.stack);
            stack.clear(ch, group, stock.storages);
            stack.set_on_empty(vec![Action::SendMessage(Message {
                component: Component::CitizenAi,
                message: 0,
                entity: None,
                a: 0,
                b: 0,
            })]);
            let material = &stock.materials[found.material];
            stack.push(
                ch,
                group,
                Action::SendMessage(Message {
                    component: Component::Citizen,
                    message: if tool {
                        1
                    } else {
                        6 + ((material.flags >> 15) & 1) as i32
                    },
                    entity: None,
                    a: if tool { material.tool_uses } else { 0 },
                    b: 0,
                }),
            );
            stock.storages[found.storage].reserve(found.material, 1);
            stack.push(
                ch,
                group,
                Action::ConsumeStoredRawMaterial {
                    storage: found.storage,
                    items: vec![(found.material, 1)],
                    reserved: true,
                },
            );
            // FUN_1400c7e70: face the source model (or map rectangle centre).
            let e = &town.entities[stock.sites[found.site].entity];
            let centre = if e.has_model {
                e.position
            } else {
                e.rect.map_or(e.position, |(x, y, w, h)| {
                    crate::town::floor_ground(
                        town.floors,
                        size,
                        terrain,
                        (y + (h >> 1)) * size as i32 + x + (w >> 1),
                    )
                })
            };
            let facing = crate::behaviour::character::work_facing(centre, found.target);
            stack.push(
                ch,
                group,
                Action::PerformWork {
                    facing,
                    anim: 8,
                    duration: ticks(1.0),
                    elapsed: 0,
                    consume: false,
                },
            );
            stack.push(
                ch,
                group,
                Action::move_to(MoveTo::new(found.target, 1, 0, WALKABLE, 0, -1.0, rng)),
            );
            ch.stack = stack;
            state.health_events |= 4;
            state.supply_activity = 0;
            state.job = -1;
            ch.stack.is_active()
        }

        activity::IDLE => {
            let cell = town.cell_of(ch.position);
            let floors = town.floors;
            let ground = |c: i32| {
                let n = size as i32;
                let h = (n >> 1) as f32 - 0.5;
                let (x, y) = ((c % n) as f32 - h, (c / n) as f32 - h);
                [x, y, floors.ground(size, terrain, x, y)]
            };
            let ground_cell = move |c: i32| crate::town::floor_ground(floors, size, terrain, c);
            let mut geometry = StorageGeometry {
                town,
                sectors,
                ground: &ground_cell,
            };
            let mut rank =
                |a: i32, b: i32| crate::work::stock::StorageSearchMap::rank(&mut geometry, a, b);
            let Some(plan) = loiter(
                town,
                cell,
                state.home,
                rules.loiter_range,
                rng,
                ground,
                &mut rank,
            ) else {
                return false;
            };
            // `FUN_1400bfb80`.
            let mut stack = std::mem::take(&mut ch.stack);
            stack.clear(ch, group, stock.storages);
            stack.set_on_empty(vec![Action::SendMessage(Message {
                component: Component::CitizenAi,
                message: 0,
                entity: None,
                a: 0,
                b: 0,
            })]);
            if let Some(e) = plan.happiness {
                // Citizen message 8 (state bit 0x40000), then HappinessComponent message 0.
                for (component, message, entity) in [
                    (Component::Citizen, 8, None),
                    (Component::Happiness, 0, Some(e)),
                ] {
                    stack.push(
                        ch,
                        group,
                        Action::SendMessage(Message {
                            component,
                            message,
                            entity,
                            a: 0,
                            b: 0,
                        }),
                    );
                }
            }
            stack.push(
                ch,
                group,
                Action::PlayAnimation {
                    anim: 0,
                    duration: ticks(plan.duration),
                    elapsed: 0,
                },
            );
            let walk = MoveTo::new(plan.target, 1, 0, WALKABLE, 0, -1.0, rng);
            stack.push(ch, group, Action::move_to(walk));
            ch.stack = stack;
            ai.since_loiter = 0;
            ch.stack.is_active()
        }
        _ => false,
    }
}

impl Agents {
    /// `FUN_1401148b0` for citizen `i`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn think(
        &mut self,
        i: usize,
        citizens: &mut Vec<Citizen>,
        env: &mut Env,
        groups: &[Option<&AnimationGroup>],
        adult: &[bool],
        entities: &[EntityView],
        dt: i64,
    ) {
        let Some(group) = groups.get(i).copied().flatten() else {
            return;
        };
        let c = &citizens[i];
        let (home_food, home_fuel) = self.states[i]
            .home
            .and_then(|h| env.construction.homes.get(&h))
            .map_or((false, false), |h| {
                (
                    crate::people::needs::home_has_food(
                        h,
                        self.states[i].home_slot,
                        env.storages,
                        env.materials,
                        &self.life,
                    ),
                    crate::people::needs::home_has_fuel(h, env.storages, env.materials, &self.life),
                )
            });
        let spouse_activity = c
            .spouse
            .map(|p| self.states[p].supply_activity)
            .filter(|&a| a == 1 || a == 2)
            .map(|a| a as u32);
        let view = CitizenView {
            home_food,
            home_fuel,
            spouse_activity,
            doctor: self.states[i].doctor.is_some(),
            age: c.age,
            school: self.states[i].school.is_some(),
            tool: self.states[i].tool_uses,
            clothing: self.states[i].clothing,
            home: self.states[i].home.is_some(),
            carrying: self.states[i].carried.used(env.materials),
            health: self.states[i].health,
            happiness: self.states[i].happiness,
            hunger: self.states[i].hunger,
            cold: self.states[i].cold,
            sick: self.states[i].sick,
            emergency: crate::disasters::fire::has_emergency(
                env.cells,
                env.placed,
                self.states[i].home,
                self.states[i].workplace,
                self.characters[i].position,
            ),
            ..Default::default()
        };
        let rules = self.rules;
        // `FUN_1401148b0`: urgently hungry with food stored at home, the citizen eats from it
        // (`FUN_14011a7b0`, after `FUN_14012fea0`'s count) before any urgent choice.
        let home_meal = if self.ais[i].hunger_branch(dt, &view, &rules.thresholds) {
            self.states[i]
                .home
                .and_then(|h| env.construction.homes.get(&h))
                .map(|h| h.storage)
                .filter(|&st| crate::people::life::stored(&env.storages[st], env.materials, 5) > 0)
                .map(|st| {
                    crate::people::life::eat(
                        &mut self.states[i],
                        &mut env.storages[st],
                        env.materials,
                        env.rng,
                        &mut env.construction.counters,
                    );
                    self.states[i].hunger
                })
        } else {
            None
        };
        let mut ai = std::mem::take(&mut self.ais[i]);
        let busy = self.characters[i].stack.is_active();
        // The AI draws from the same stream the activities do.
        let mut rng = std::mem::replace(env.rng, Random::new(0));
        ai.update(
            dt,
            &view,
            &rules.thresholds,
            &mut rng,
            home_meal,
            || busy,
            |ai, a, rng| {
                std::mem::swap(env.rng, rng);
                // `FUN_14011ada0` once for the home-stocking activities.
                let home_reachable = (a == activity::HOME_FOOD || a == activity::HOME_FUEL) && {
                    let town = TownView {
                        size: env.cells.size,
                        types: &env.cells.types,
                        regions: &self.regions,
                        occupants: &env.cells.occupants,
                        objects: &env.cells.objects,
                        floors: &env.cells.floors,
                        entities,
                        happiness: &env.construction.happiness,
                    };
                    let cell = town.cell_of(self.characters[i].position);
                    self.states[i]
                        .home
                        .is_some_and(|h| town.can_reach(cell, h, env.rng))
                };
                let carrying = self.states[i].carried.used(env.materials) > 0;
                let done = if (home_reachable || a == activity::HERBS) && carrying {
                    // `FUN_140111160` / `FUN_1401108b0` / `FUN_140111d60`: first drop off what
                    // is carried, then this activity again.
                    let (mut sim, jobs) = self.sim(citizens, env, groups, adult, entities);
                    sim.drop_off(i);
                    jobs.drain(&mut sim);
                    ai.queued = Some(a);
                    true
                } else if matches!(
                    a,
                    activity::EAT
                        | activity::EAT_URGENT
                        | activity::WARM
                        | activity::WARM_URGENT
                        | activity::HOME_FOOD
                        | activity::HOME_FUEL
                        | activity::HERBS
                        | activity::DOCTOR
                        | activity::SCHOOL
                        | activity::PLAY
                ) {
                    let town = TownView {
                        size: env.cells.size,
                        types: &env.cells.types,
                        regions: &self.regions,
                        occupants: &env.cells.occupants,
                        objects: &env.cells.objects,
                        floors: &env.cells.floors,
                        entities,
                        happiness: &env.construction.happiness,
                    };
                    let mut needs = crate::people::needs::Needs {
                        town: &town,
                        sectors: &mut self.sectors,
                        sites: env.sites,
                        storages: env.storages,
                        materials: env.materials,
                        homes: &mut env.construction.homes,
                        residences: &env.construction.workforce.residences,
                        temperature: env.temperature,
                        life: &self.life,
                        carry: self.carry,
                        terrain: env.terrain,
                        size: env.cells.size,
                        rng: env.rng,
                        healers: &env.construction.healers,
                        workplaces: &env.construction.workforce.workplaces,
                        attendance: &mut env.construction.attendance,
                        citizens,
                        reschool: false,
                    };
                    let mut who = crate::people::needs::Who {
                        ai,
                        ch: &mut self.characters[i],
                        state: &mut self.states[i],
                        group,
                    };
                    let done = match a {
                        activity::EAT | activity::EAT_URGENT => needs.eat(&mut who),
                        activity::WARM | activity::WARM_URGENT => needs.warm(&mut who),
                        activity::HOME_FOOD => needs.stock_home(&mut who, false, home_reachable),
                        activity::HOME_FUEL => needs.stock_home(&mut who, true, home_reachable),
                        activity::HERBS => needs.herbs(&mut who),
                        activity::DOCTOR => needs.doctor(&mut who, i),
                        activity::SCHOOL => needs.school(&mut who),
                        _ => needs.play(&mut who, i),
                    };
                    if needs.reschool {
                        // `FUN_14011af20(c, 1)`.
                        let mut born = Vec::new();
                        self.with_people(
                            citizens,
                            env,
                            entities,
                            &mut |_, m, _| m.clone(),
                            &mut born,
                            |p| p.leave_service(i, crate::people::life::service::SCHOOL, true),
                        );
                    }
                    let outbox = std::mem::take(&mut self.characters[i].stack.outbox);
                    if !outbox.is_empty() {
                        let (mut sim, jobs) = self.sim(citizens, env, groups, adult, entities);
                        for m in outbox {
                            jobs.fail(m.a, i, m.b, &mut sim);
                        }
                        jobs.drain(&mut sim);
                    }
                    done
                } else if a == activity::DROP_OFF {
                    // `FUN_14011b330`.
                    let (mut sim, jobs) = self.sim(citizens, env, groups, adult, entities);
                    sim.drop_off(i);
                    jobs.drain(&mut sim);
                    self.characters[i].stack.is_active()
                } else if a == activity::WORK || a == activity::EMERGENCY {
                    let (mut sim, jobs) = self.sim(citizens, env, groups, adult, entities);
                    if a == activity::WORK {
                        jobs.assign_work(i, &mut sim);
                    } else {
                        jobs.assign_emergency(i, &mut sim);
                    }
                    jobs.drain(&mut sim);
                    self.characters[i].stack.is_active()
                } else {
                    let town = TownView {
                        size: env.cells.size,
                        types: &env.cells.types,
                        regions: &self.regions,
                        occupants: &env.cells.occupants,
                        objects: &env.cells.objects,
                        floors: &env.cells.floors,
                        entities,
                        happiness: &env.construction.happiness,
                    };
                    let mut stock = TownStock {
                        materials: env.materials,
                        storages: env.storages,
                        sites: env.sites,
                    };
                    let ch = &mut self.characters[i];
                    let done = run(
                        ai,
                        a,
                        ch,
                        &mut self.states[i],
                        &town,
                        &mut self.sectors,
                        &mut stock,
                        env.terrain,
                        group,
                        rules,
                        env.cells.size,
                        env.rng,
                    );
                    // Aborted CompleteTasks of the replaced sequence.
                    let outbox = std::mem::take(&mut ch.stack.outbox);
                    if !outbox.is_empty() {
                        let (mut sim, jobs) = self.sim(citizens, env, groups, adult, entities);
                        for m in outbox {
                            jobs.fail(m.a, i, m.b, &mut sim);
                        }
                        jobs.drain(&mut sim);
                    }
                    done
                };
                std::mem::swap(env.rng, rng);
                done
            },
        );
        *env.rng = rng;
        self.ais[i] = ai;
    }
}
