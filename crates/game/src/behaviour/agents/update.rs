//! The agents' iteration: characters, animals and hunt messages.

mod animals;

use super::*;

impl Agents {
    /// The bucket visits in `visits` split into the placed entities' (sites, piles) and the
    /// citizens' (from `EXTRA`, as citizen indices), each in visiting order.
    pub fn split_visits(visits: &[(usize, i64)]) -> (Vec<(usize, i64)>, Vec<(usize, i64)>) {
        let (extra, foreign) = (crate::scheduler::EXTRA, crate::scheduler::FOREIGN);
        let sites = visits.iter().copied().filter(|&(v, _)| v < extra).collect();
        let citizens = visits
            .iter()
            .filter(|&&(v, _)| (extra..foreign).contains(&v))
            .map(|&(v, e)| (v - extra, e))
            .collect();
        (sites, citizens)
    }

    /// The citizens' part of an iteration's every-iteration phase (bit `0x04`): the town step,
    /// the job icons, every citizen's character (animation, actions, movement), then the sites'
    /// state machines, nomads, boats and tornadoes. `groups[i]` is citizen `i`'s animation group
    /// (`None` for the dead); `newborn` makes a child, with its random draws.
    ///
    /// Original: `FUN_1400c9560` (the bit-`0x04` set).
    pub fn every_iteration(
        &mut self,
        citizens: &mut Vec<Citizen>,
        env: &mut Env,
        groups: &[Option<&AnimationGroup>],
        dt: f32,
        newborn: &mut dyn FnMut(&mut Random, &Citizen, bool) -> Citizen,
    ) {
        self.sync_map(env.cells);
        // The town step (`FUN_140091780`): the population, and the visitors' disease risk
        // running out.
        {
            let c = &mut *env.construction;
            c.population = self.registry.len() as i32;
            c.visitors.1 -= ticks(dt);
            if c.visitors.1 < 0 {
                c.visitors = (0.0, 0);
            }
            self.town_disease = c.visitors.0;
        }
        // `FUN_140118340`'s job icon (bit 3): no workplace for a profession that needs one.
        let professions = env.construction.professions.clone();
        for state in &mut self.states {
            let anywhere = if professions.list.is_empty() {
                self.rules.profession_allows_idle
            } else {
                professions.works_anywhere(state.profession)
            };
            crate::people::status::set_icon(
                &mut state.status,
                3,
                !anywhere && state.workplace.is_none(),
                false,
            );
        }
        self.sectors.age_cache(ticks(dt));
        let mut entities = entity_views(env.placed, env.templates, env.construction);
        let mut animating = crate::town::animating(env.placed, env.templates);
        let adult = self.adults(citizens);
        for (ch, group) in self.characters.iter_mut().zip(groups) {
            if let Some(g) = group {
                let position = ch.position;
                let fired = ch.animate(g, dt);
                self.sounds.fire(&fired, position, env.rng);
            }
        }
        // The citizens' CharacterComponents in the characters' tree: the dead leave it
        // (`FUN_14010ee50`), the new join it (`FUN_14010ee90`), in index order.
        // Births, bands and deaths join and leave it on the spot (`join_tree`, `remove`); this
        // catches the start's citizens and any made elsewhere.
        for i in 0..self.characters.len() {
            let alive = groups.get(i).copied().flatten().is_some()
                && citizens.get(i).is_some_and(|c| c.alive);
            let ch = &mut self.characters[i];
            if !alive && 0 <= ch.tree_slot {
                env.wildlife.tree.remove(ch.tree_slot);
                ch.tree_slot = -1;
            }
        }
        for i in 0..self.characters.len() {
            let alive = groups.get(i).copied().flatten().is_some()
                && citizens.get(i).is_some_and(|c| c.alive);
            let ch = &mut self.characters[i];
            if alive && ch.tree_slot < 0 {
                ch.tree_slot = env.wildlife.tree.insert(
                    crate::behaviour::character_tree::Who::Citizen(i),
                    ch.position,
                    ch.desc.radius,
                );
            }
        }
        for i in 0..self.characters.len() {
            let Some(group) = groups[i] else { continue };
            let mut messages = Vec::new();
            let mut used = Vec::new();
            {
                let state = &mut self.states[i];
                // The current job's work (`+0x30`) and its profession's accident odds.
                let accident_odds = (0 <= state.job)
                    .then(|| env.construction.jobs.works.get(&state.job))
                    .flatten()
                    .and_then(|w| {
                        let p = env.construction.professions.list.get(w.profession)?;
                        Some((w.profession, p.accident.0, p.accident.1))
                    });
                // `Town::FindMaterialStorage` for a redirected drop-off.
                let town = crate::behaviour::activities::TownView {
                    size: env.cells.size,
                    types: &env.cells.types,
                    regions: &self.regions,
                    occupants: &env.cells.occupants,
                    objects: &env.cells.objects,
                    floors: &env.cells.floors,
                    entities: &entities,
                    happiness: &env.construction.happiness,
                };
                let (size, terrain, sites, materials) =
                    (env.cells.size, env.terrain, env.sites, env.materials);
                let floors = &env.cells.floors;
                let sectors = &mut self.sectors;
                let mut search = |storages: &[Storage],
                                  rng: &mut Random,
                                  m: usize,
                                  n: i32,
                                  cell: i32|
                 -> Option<(usize, [f32; 3])> {
                    let ground =
                        move |cell: i32| crate::town::floor_ground(floors, size, terrain, cell);
                    let mut geometry = crate::behaviour::activities::StorageGeometry {
                        town: &town,
                        sectors: &mut *sectors,
                        ground: &ground,
                    };
                    crate::work::stock::find_material_storage(
                        sites,
                        storages,
                        materials,
                        m,
                        n,
                        true,
                        cell,
                        None,
                        &mut geometry,
                        rng,
                    )
                    .map(|f| (f.storage, f.target))
                };
                let mut world = World {
                    size: env.cells.size,
                    types: &env.cells.types,
                    objects: &env.cells.objects,
                    occupants: &env.cells.occupants,
                    flags: &env.cells.flags,
                    roads: &env.cells.road_kind,
                    floors: &env.cells.floors,
                    regions: &self.regions,
                    terrain: env.terrain,
                    pathfinder: &mut self.pathfinder,
                    rng: env.rng,
                    messages: &mut messages,
                    used: &mut used,
                    materials: env.materials,
                    storages: env.storages,
                    storage_search: Some(&mut search),
                    pushes: Vec::new(),
                    carried: &mut state.carried,
                    tool_uses: &mut state.tool_uses,
                    shown_tool: &mut state.shown_tool,
                    animating: &mut animating,
                    accident: &mut state.accident,
                    accident_timer: &mut state.accident_timer,
                    accident_odds,
                };
                self.characters[i].update(&mut world, group, dt);
                // `FUN_14010e990` ends by placing it in the tree again (`FUN_140019070`).
                let ch = &self.characters[i];
                env.wildlife
                    .tree
                    .place(ch.tree_slot, ch.position, ch.desc.radius);
                env.wildlife.tree.set_direction(ch.tree_slot, ch.direction);
            }
            for (m, n) in used {
                if let Some(slot) = crate::work::produce::production_group(env.materials[m].flags) {
                    env.construction.counters.used[slot] += n;
                }
            }
            citizens[i].position = self.characters[i].position;
            self.deliver(i, messages, citizens, env, groups, &adult, &mut entities);
        }
        // The sites' state machines, nomads, boats and tornadoes (bit 0x04).
        {
            let (mut sim, jobs) = self.sim(citizens, env, groups, &adult, &entities);
            sim.update_sites_every(jobs, ticks(dt));
            sim.update_nomads(ticks(dt));
            sim.update_boats();
            sim.update_tornadoes(jobs, ticks(dt));
            jobs.drain(&mut sim);
        }
        // The tornadoes' victims (`FUN_14011c2a0(citizen, 5)` from `FUN_140058d20`).
        for c in std::mem::take(&mut env.construction.tornado_kills) {
            if !citizens[c].alive {
                continue;
            }
            let mut born = Vec::new();
            self.with_people(citizens, env, &entities, newborn, &mut born, |people| {
                people.die(c, crate::people::life::reason::TORNADO, None);
            });
        }
    }

    /// Bucket 3's visits (sites and piles), in visiting order, each with the time since its last
    /// visit: the state machines' scheduled update, attendance, the piles' ages, hunts, blight,
    /// fires, the residences' storage and the homes. `dt` is the iteration's time.
    ///
    /// Original: `FUN_1400c9560` (bucket 3).
    pub fn visit_sites(
        &mut self,
        citizens: &mut [Citizen],
        env: &mut Env,
        groups: &[Option<&AnimationGroup>],
        dt: f32,
        site_visits: &[(usize, i64)],
    ) {
        let entities = entity_views(env.placed, env.templates, env.construction);
        let adult = self.adults(citizens);
        {
            let (mut sim, jobs) = self.sim(citizens, env, groups, &adult, &entities);
            sim.update_sites_visits(jobs, ticks(dt), site_visits);
            sim.update_attendance(site_visits);
            sim.age_piles(jobs, site_visits);
            sim.update_hunts(site_visits);
            sim.update_blight(site_visits);
            sim.update_fires(jobs, site_visits);
            jobs.drain(&mut sim);
        }
        // The residences' state-2 updates (`FUN_14012ee40`): the ClearStorage work, then the
        // fire.
        {
            let (mut sim, jobs) = self.sim(citizens, env, groups, &adult, &entities);
            sim.residence_clear_storage(jobs, site_visits);
            jobs.drain(&mut sim);
        }
        {
            let c = &mut *env.construction;
            for &(e, elapsed) in site_visits {
                let Some(home) = c.homes.get_mut(&e) else {
                    continue;
                };
                crate::people::life::update_home(
                    home,
                    env.storages,
                    env.materials,
                    env.temperature,
                    elapsed,
                    &mut c.counters,
                );
            }
        }
    }

    /// Bucket 1's visits (the citizens, as citizen indices), in visiting order, each with the time
    /// since its last visit: the character's notify, CitizenComponent, then CitizenAIComponent.
    ///
    /// Original: `FUN_1400c9560` (bucket 1).
    pub fn visit_citizens(
        &mut self,
        citizens: &mut Vec<Citizen>,
        env: &mut Env,
        groups: &[Option<&AnimationGroup>],
        citizen_visits: &[(usize, i64)],
        newborn: &mut dyn FnMut(&mut Random, &Citizen, bool) -> Citizen,
    ) {
        let mut entities = entity_views(env.placed, env.templates, env.construction);
        let adult = self.adults(citizens);
        // The citizens bucket 1 visited this iteration, in its order, each with the time since
        // its last visit (`FUN_1400c9560`).
        for &(i, elapsed) in citizen_visits {
            if i >= self.ais.len()
                || groups.get(i).copied().flatten().is_none()
                || !citizens[i].alive
            {
                continue;
            }
            // CharacterComponent's scheduled update (`FUN_14010e950`): the running chain's
            // notify, a hunter's `HuntAnimal`.
            refresh_views(&mut entities, env);
            self.hunt_notify(i, env, groups);
            let following = env.wildlife.tree.follows(self.characters[i].tree_slot);
            self.characters[i].stack.notify_moves(following);
            // CitizenComponent before CitizenAIComponent (the template's component order); a
            // citizen queued for deletion stops there (entity flag `0x400`).
            let born = self.live(i, citizens, env, &entities, elapsed, newborn);
            for c in born {
                let mut ch = Character::new(self.desc, citizens[c].position, CHILD_SCALE);
                ch.yaw = self.characters[i].yaw;
                self.characters.push(ch);
                self.ais.push(CitizenAi::default());
                self.states[c].carried = self.carried.clone();
                // The child's entity activates as it is made (`FUN_14010ee90`).
                self.join_tree(c, env);
            }
            if self.dying.iter().any(|&(d, _)| d == i) {
                continue;
            }
            refresh_views(&mut entities, env);
            self.think(i, citizens, env, groups, &adult, &entities, elapsed);
        }
    }

    /// The end of an iteration, after every bucket: destroyed sites leave, the burials' tombstones
    /// are placed, and the queued citizens are deleted (each component's remove slot).
    ///
    /// Original: `FUN_1400c9560` (its deletions).
    pub fn finish(
        &mut self,
        citizens: &mut Vec<Citizen>,
        env: &mut Env,
        groups: &[Option<&AnimationGroup>],
    ) {
        let entities = entity_views(env.placed, env.templates, env.construction);
        let adult = self.adults(citizens);
        {
            let (mut sim, jobs) = self.sim(citizens, env, groups, &adult, &entities);
            sim.remove_destroyed_sites(jobs);
            sim.place_burials();
            jobs.drain(&mut sim);
        }
        // The entity manager deletes the queued entities (each component's remove slot);
        // removals can queue more (orphaned children).
        let mut k = 0;
        while k < self.dying.len() {
            let (c, _) = self.dying[k];
            self.remove(c, citizens, env, groups, &entities);
            k += 1;
        }
        self.dying.clear();
        for (c, s) in citizens.iter_mut().zip(&self.states) {
            c.school = s.school;
        }
    }
}
