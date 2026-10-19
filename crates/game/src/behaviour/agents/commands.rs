//! Commands from the town: new citizens, tree slots and newcomers arriving.

use super::*;

impl Agents {
    /// Player commands and newly placed sites, applied before an iteration (where the
    /// original's UI events and placement land between iterations).
    pub fn apply_commands(
        &mut self,
        citizens: &mut Vec<Citizen>,
        env: &mut Env,
        groups: &[Option<&AnimationGroup>],
        newborn: &mut dyn FnMut(&mut Random, &Citizen, bool) -> Citizen,
    ) {
        self.sync_map(env.cells);
        let commands = std::mem::take(&mut env.construction.commands);
        let placed = std::mem::take(&mut env.construction.placed);
        let destroy = std::mem::take(&mut env.construction.destroy);
        if commands.is_empty() && placed.is_empty() && destroy.is_empty() {
            return;
        }
        let entities = entity_views(env.placed, env.templates, env.construction);
        let adult = self.adults(citizens);
        // A band taken in (ImmigrationUI `0x9858`) becomes citizens where its command stands;
        // the commands after it run once it has.
        let mut admission = None;
        let mut road_markers = Vec::new();
        let (mut sim, jobs) = self.sim(citizens, env, groups, &adult, &entities);
        // Placing activates at once in the original (`FUN_1400c9d70`); a cancel comes after.
        for e in placed {
            sim.activate(jobs, e);
            jobs.drain(&mut sim);
        }
        for e in destroy {
            sim.destroy_entity(jobs, e);
            jobs.drain(&mut sim);
        }
        let mut queue = commands.into_iter();
        while let Some(command) = queue.next() {
            match command {
                crate::work::professions::Command::SetCrop { entity, crop } => {
                    if sim.wildlife.pastures.contains_key(&entity) {
                        // The kind's list is only enabled while the pasture is empty
                        // (`FUN_1400ba320`).
                        if sim.wildlife.pastures[&entity].livestock.is_empty() {
                            sim.set_pasture_kind(jobs, entity, crop);
                        }
                    } else if sim.sites.get(&entity).is_some_and(|s| s.orchard.is_some()) {
                        sim.set_orchard_tree(entity, crop);
                    } else {
                        sim.set_crop(entity, crop);
                    }
                    jobs.drain(&mut sim);
                    continue;
                }
                crate::work::professions::Command::FieldAction { entity, action } => {
                    if sim.wildlife.pastures.contains_key(&entity) {
                        sim.pasture_action(jobs, entity, action);
                    } else if sim.sites.get(&entity).is_some_and(|s| s.orchard.is_some()) {
                        sim.orchard_action(entity, action);
                    } else {
                        sim.field_action(entity, action);
                    }
                    jobs.drain(&mut sim);
                    continue;
                }
                crate::work::professions::Command::SetLimit { entity, limit } => {
                    sim.set_pasture_limit(entity, limit);
                    continue;
                }
                crate::work::professions::Command::SetEnabled { entity, enabled } => {
                    sim.enable_workplace(entity, enabled);
                    jobs.drain(&mut sim);
                    continue;
                }
                crate::work::professions::Command::PlanRoad {
                    cell,
                    kind,
                    ty,
                    material,
                } => {
                    sim.plan_road(jobs, cell, kind, ty, material);
                    jobs.drain(&mut sim);
                    continue;
                }
                crate::work::professions::Command::ClearRoad { cell, marker } => {
                    if sim.unplan_road(jobs, cell) {
                        road_markers.push((cell, marker));
                    }
                    jobs.drain(&mut sim);
                    continue;
                }
                crate::work::professions::Command::Demolish { entity } => {
                    sim.demolish(jobs, entity);
                    jobs.drain(&mut sim);
                    continue;
                }
                crate::work::professions::Command::StateMessage { entity, message } => {
                    if message == 7 {
                        if let Some(band) = sim.take_band(entity) {
                            admission = Some((entity, band));
                            break;
                        }
                    }
                    sim.destroy_message(jobs, entity, message);
                    sim.immigration_message(entity, message);
                    sim.trade_message(entity, message);
                    jobs.drain(&mut sim);
                    continue;
                }
                crate::work::professions::Command::Control {
                    entity,
                    kind,
                    index,
                } => {
                    sim.control(jobs, entity, kind, index);
                    jobs.drain(&mut sim);
                    continue;
                }
                crate::work::professions::Command::SetRecipe { entity, recipe } => {
                    if let Some(p) = sim.sites.get_mut(&entity).and_then(|s| s.produce.as_mut()) {
                        p.selector = recipe;
                    }
                    continue;
                }
                crate::work::professions::Command::Exchange { entity, sell, buy } => {
                    sim.exchange(entity, &sell, &buy);
                    continue;
                }
                crate::work::professions::Command::Trade { entity, setting } => {
                    sim.trade_setting(entity, setting);
                    continue;
                }
                crate::work::professions::Command::Upgrade { entity, template } => {
                    sim.upgrade(jobs, entity, template);
                    jobs.drain(&mut sim);
                    continue;
                }
                crate::work::professions::Command::CancelDemolition { entity } => {
                    sim.cancel_demolition(jobs, entity);
                    jobs.drain(&mut sim);
                    continue;
                }
                _ => {}
            }
            let mut workers = sim.workers();
            let town = TownView {
                size: sim.size,
                types: &sim.cells.types,
                regions: sim.regions,
                occupants: &sim.cells.occupants,
                objects: &sim.cells.objects,
                floors: &sim.cells.floors,
                entities: sim.entities,
                happiness: sim.happiness,
            };
            let (size, terrain) = (sim.size, sim.terrain);
            let floors = &sim.cells.floors;
            let ground = move |c: i32| crate::town::floor_ground(floors, size, terrain, c);
            let mut geometry = StorageGeometry {
                town: &town,
                sectors: sim.sectors,
                ground: &ground,
            };
            let characters = &*sim.characters;
            let cell = |c: usize| town.cell_of(characters[c].position);
            let professions = sim.professions;
            let anywhere = |p: usize| professions.works_anywhere(p);
            let sites = &*sim.sites;
            let priority = |e: usize| {
                sites
                    .get(&e)
                    .and_then(|s| s.work_priority)
                    .unwrap_or(i32::MAX)
            };
            let mut search = crate::behaviour::activities::TownSearch {
                town: &town,
                cell: &cell,
                anywhere: &anywhere,
                priority: &priority,
                rng: &mut *sim.rng,
            };
            sim.workforce.apply(
                command,
                &mut workers,
                sim.professions,
                sim.homes,
                &mut geometry,
                &mut search,
            );
            sim.set_workers(&workers);
        }
        let rest: Vec<_> = queue.collect();
        env.construction.road_markers.extend(road_markers);
        if let Some((entity, band)) = admission {
            self.admit_band(citizens, env, groups, entity, &band, newborn);
            if !rest.is_empty() {
                let later = std::mem::replace(&mut env.construction.commands, rest);
                env.construction.commands.extend(later);
                self.apply_commands(citizens, env, groups, newborn);
            }
        }
    }

    /// Citizen `i`'s place in the characters' tree, put in by whoever made it.
    pub fn set_tree_slot(&mut self, i: usize, slot: i32) {
        if let Some(ch) = self.characters.get_mut(i) {
            ch.tree_slot = slot;
        }
    }

    /// CharacterComponent activation (`FUN_14010ee90`): citizen `i` joins the characters' tree.
    pub(super) fn join_tree(&mut self, i: usize, env: &mut Env) {
        let ch = &mut self.characters[i];
        if ch.tree_slot < 0 {
            ch.tree_slot = env.wildlife.tree.insert(
                crate::behaviour::character_tree::Who::Citizen(i),
                ch.position,
                ch.desc.radius,
            );
        }
    }

    /// `FUN_14004ddb0`: the band becomes citizens in fours where each nomad stands
    /// (`FUN_140050b00` → `FUN_14011c550`, laborers): a man, a woman (married to him), then
    /// a girl and a boy in her care. Adults are `+0x50 + RandFloat × (+0x54 − +0x50)` years old,
    /// children `RandFloat × +0x50`. Then the nomads are gone and the town hall goes back to 2.
    /// It runs in its command's turn (`apply_commands`).
    pub(super) fn admit_band(
        &mut self,
        citizens: &mut Vec<Citizen>,
        env: &mut Env,
        groups: &[Option<&AnimationGroup>],
        entity: usize,
        band: &[(usize, [f32; 3])],
        newborn: &mut dyn FnMut(&mut Random, &Citizen, bool) -> Citizen,
    ) {
        env.construction.nomads_admitted += band.len() as i32;
        let laborer = env.construction.professions.laborer;
        let r = self.life.clone();
        let (lo, hi) = (r.f(0x50), r.f(0x54));
        let mut make = |agents: &mut Self,
                        citizens: &mut Vec<Citizen>,
                        rng: &mut Random,
                        position: [f32; 3],
                        male: bool,
                        guardian: Option<usize>| {
            let child = guardian.is_some();
            let age = if child {
                rng.rand_float() * lo
            } else {
                rng.rand_float() * (hi - lo) + lo
            };
            let _flag = rng.rand_below(1);
            let template = Citizen {
                position,
                ..citizens.first().cloned().unwrap_or_else(|| Citizen {
                    name: String::new(),
                    male,
                    age: 0.0,
                    lifespan: 0.0,
                    appearance: 0,
                    educated: false,
                    education: 0,
                    position,
                    spouse: None,
                    parent: None,
                    school: None,
                    alive: true,
                })
            };
            let mut c = newborn(rng, &template, male);
            c.position = position;
            c.age = age;
            c.alive = true;
            let id = citizens.len();
            citizens.push(c);
            let mut state = CitizenState::newborn(&r);
            state.profession = laborer;
            state.carried = agents.carried.clone();
            agents.states.push(state);
            agents.registry.push(id);
            let scale = if child { CHILD_SCALE } else { 1.0 };
            agents
                .characters
                .push(Character::new(agents.desc, position, scale));
            agents.ais.push(CitizenAi::default());
            if let Some(g) = guardian {
                agents.states[g].dependents.push(id);
                citizens[id].parent = Some(g);
            }
            id
        };
        let first = citizens.len();
        let mut k = 0;
        while k < band.len() {
            let man = make(self, citizens, env.rng, band[k].1, true, None);
            if band.len() <= k + 1 {
                break;
            }
            let woman = make(self, citizens, env.rng, band[k + 1].1, false, None);
            citizens[man].spouse = Some(woman);
            citizens[woman].spouse = Some(man);
            if band.len() <= k + 2 {
                break;
            }
            make(self, citizens, env.rng, band[k + 2].1, false, Some(woman));
            if band.len() <= k + 3 {
                break;
            }
            make(self, citizens, env.rng, band[k + 3].1, true, Some(woman));
            k += 4;
        }
        // Each new citizen's entity activates as it is made (`FUN_14010ee90`), before the
        // nomads go.
        for i in first..citizens.len() {
            self.join_tree(i, env);
        }
        let entities = entity_views(env.placed, env.templates, env.construction);
        let adult = self.adults(citizens);
        let (mut sim, jobs) = self.sim(citizens, env, groups, &adult, &entities);
        sim.band_taken(entity);
        jobs.drain(&mut sim);
    }
}
