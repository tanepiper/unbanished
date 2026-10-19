//! The agents' iteration: characters, animals and hunt messages.

use super::*;

impl Agents {
    /// One simulation iteration of every citizen. `groups[i]` is citizen `i`'s animation group
    /// (`None` for the dead); `newborn` makes a child, with its random draws.
    pub fn update(
        &mut self,
        citizens: &mut Vec<Citizen>,
        env: &mut Env,
        groups: &[Option<&AnimationGroup>],
        dt: f32,
        newborn: &mut dyn FnMut(&mut Random, &Citizen, bool) -> Citizen,
    ) {
        self.sync_map(env.cells);
        // This iteration's bucket visits: placed entities, and the citizens (from `EXTRA`).
        let (extra, foreign) = (crate::scheduler::EXTRA, crate::scheduler::FOREIGN);
        let site_visits: Vec<(usize, i64)> = env
            .visits
            .iter()
            .copied()
            .filter(|&(v, _)| v < extra)
            .collect();
        let citizen_visits: Vec<(usize, i64)> = env
            .visits
            .iter()
            .filter(|&&(v, _)| (extra..foreign).contains(&v))
            .map(|&(v, e)| (v - extra, e))
            .collect();
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
                ch.animate(g, dt);
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
        // The sites' StateMachine updates (bit 4, then their bucket); the piles' ages
        // (RawMaterialComponent's scheduled update, run every iteration here).
        {
            let (mut sim, jobs) = self.sim(citizens, env, groups, &adult, &entities);
            sim.update_sites(jobs, ticks(dt), &site_visits);
            sim.update_nomads(ticks(dt));
            sim.update_boats();
            sim.update_attendance(&site_visits);
            sim.age_piles(jobs, &site_visits);
            sim.update_hunts(&site_visits);
            sim.update_blight(&site_visits);
            sim.update_fires(jobs, &site_visits);
            sim.update_tornadoes(jobs, ticks(dt));
            sim.place_burials();
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
        // The residences' state-2 updates (`FUN_14012ee40`): the ClearStorage work, then the
        // fire.
        {
            let (mut sim, jobs) = self.sim(citizens, env, groups, &adult, &entities);
            sim.residence_clear_storage(jobs, &site_visits);
            jobs.drain(&mut sim);
        }
        {
            let c = &mut *env.construction;
            for &(e, elapsed) in &site_visits {
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
        // The citizens bucket 1 visited this iteration, in its order, each with the time since
        // its last visit (`FUN_1400c9560`).
        for &(i, elapsed) in &citizen_visits {
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

    /// The wild animals' character update: animate, run the actions, turn and move.
    ///
    /// Deviation: the entity manager runs these updates in the order of a tree keyed by the
    /// entity's address; the port takes them in the order they were made, which a growing heap
    /// usually gives too.
    ///
    /// Original: `FUN_14010e990`; the order comes from `FUN_1400c9560`, `FUN_1400c82b0` and the
    /// tree at manager `+0x68` (`FUN_1400c90a0`).
    pub fn move_animals(
        &mut self,
        wildlife: &mut crate::nature::wildlife::Wildlife,
        cells: &Cells,
        terrain: &HeightGrid,
        rng: &mut Random,
        materials: &[RawMaterial],
        dt: f32,
    ) {
        self.sync_map(cells);
        let mut animating = Vec::new();
        let mut storages: Vec<Storage> = Vec::new();
        let ids: Vec<usize> = wildlife.animals.keys().copied().collect();
        for a in ids {
            let (template, variant) = {
                let animal = &wildlife.animals[&a];
                if animal.dying {
                    continue;
                }
                (animal.template, animal.variant)
            };
            let Some(group) = wildlife.groups.get(&(template, variant)).cloned() else {
                continue;
            };
            let animal = wildlife.animals.get_mut(&a).unwrap();
            let mut messages = Vec::new();
            let (mut carried, mut tool_uses, mut shown_tool) = (Storage::default(), 0, -1);
            let (mut accident, mut accident_timer) = (None, 0);
            let mut world = World {
                size: cells.size,
                types: &cells.types,
                objects: &cells.objects,
                occupants: &cells.occupants,
                flags: &cells.flags,
                roads: &cells.road_kind,
                floors: &cells.floors,
                regions: &self.regions,
                terrain,
                pathfinder: &mut self.pathfinder,
                rng,
                messages: &mut messages,
                used: &mut Vec::new(),
                materials,
                storages: &mut storages,
                carried: &mut carried,
                tool_uses: &mut tool_uses,
                storage_search: None,
                pushes: Vec::new(),
                shown_tool: &mut shown_tool,
                animating: &mut animating,
                accident: &mut accident,
                accident_timer: &mut accident_timer,
                accident_odds: None,
            };
            animal.character.animate(&group, dt);
            animal.character.update(&mut world, &group, dt);
            let ch = &animal.character;
            wildlife
                .tree
                .place(ch.tree_slot, ch.position, ch.desc.radius);
            wildlife.tree.set_direction(ch.tree_slot, ch.direction);
        }
        // The trader boats' CharacterComponent.
        let ids: Vec<usize> = wildlife.boats.keys().copied().collect();
        for id in ids {
            let b = &wildlife.boats[&id];
            let Some(group) = wildlife.groups.get(&(b.template, 0)).cloned() else {
                continue;
            };
            let boat = wildlife.boats.get_mut(&id).unwrap();
            let mut messages = Vec::new();
            let (mut carried, mut tool_uses, mut shown_tool) = (Storage::default(), 0, -1);
            let (mut accident, mut accident_timer) = (None, 0);
            let mut world = World {
                size: cells.size,
                types: &cells.types,
                objects: &cells.objects,
                occupants: &cells.occupants,
                flags: &cells.flags,
                roads: &cells.road_kind,
                floors: &cells.floors,
                regions: &self.regions,
                terrain,
                pathfinder: &mut self.pathfinder,
                rng,
                messages: &mut messages,
                used: &mut Vec::new(),
                materials,
                storages: &mut storages,
                carried: &mut carried,
                tool_uses: &mut tool_uses,
                storage_search: None,
                pushes: Vec::new(),
                shown_tool: &mut shown_tool,
                animating: &mut animating,
                accident: &mut accident,
                accident_timer: &mut accident_timer,
                accident_odds: None,
            };
            boat.character.animate(&group, dt);
            boat.character.update(&mut world, &group, dt);
            let ch = &boat.character;
            wildlife
                .tree
                .place(ch.tree_slot, ch.position, ch.desc.radius);
            wildlife.tree.set_direction(ch.tree_slot, ch.direction);
        }
        // The nomads' CharacterComponent.
        let ids: Vec<usize> = wildlife.nomads.keys().copied().collect();
        for id in ids {
            let n = &wildlife.nomads[&id];
            let Some(group) = wildlife.groups.get(&(n.template, n.variant)).cloned() else {
                continue;
            };
            let nomad = wildlife.nomads.get_mut(&id).unwrap();
            let mut messages = Vec::new();
            let (mut carried, mut tool_uses, mut shown_tool) = (Storage::default(), 0, -1);
            let (mut accident, mut accident_timer) = (None, 0);
            let mut world = World {
                size: cells.size,
                types: &cells.types,
                objects: &cells.objects,
                occupants: &cells.occupants,
                flags: &cells.flags,
                roads: &cells.road_kind,
                floors: &cells.floors,
                regions: &self.regions,
                terrain,
                pathfinder: &mut self.pathfinder,
                rng,
                messages: &mut messages,
                used: &mut Vec::new(),
                materials,
                storages: &mut storages,
                carried: &mut carried,
                tool_uses: &mut tool_uses,
                storage_search: None,
                pushes: Vec::new(),
                shown_tool: &mut shown_tool,
                animating: &mut animating,
                accident: &mut accident,
                accident_timer: &mut accident_timer,
                accident_odds: None,
            };
            nomad.character.animate(&group, dt);
            nomad.character.update(&mut world, &group, dt);
            let ch = &nomad.character;
            wildlife
                .tree
                .place(ch.tree_slot, ch.position, ch.desc.radius);
            wildlife.tree.set_direction(ch.tree_slot, ch.direction);
        }
    }

    /// `HuntAnimal`'s notify (`FUN_1400cd2b0`): once the lodge has gathered its hunting time
    /// (`FUN_140127f50`), the nearest wild animal within 8 (`FUN_140097730`), not already
    /// taken, next to a cell the hunter can reach, is taken (`FUN_1400c1370` without clearing:
    /// walk there, shoot 4 s, remove it, leave its raw materials where it stood), and its herd
    /// moves on (`FUN_140127810`).
    pub(super) fn hunt_notify(
        &mut self,
        i: usize,
        env: &mut Env,
        groups: &[Option<&AnimationGroup>],
    ) {
        let Some(group) = groups.get(i).copied().flatten() else {
            return;
        };
        let Some((lodge, done)) = self.characters[i].stack.hunt_watch() else {
            return;
        };
        if *done {
            return;
        }
        let Some(site) = env.construction.sites.get(&lodge) else {
            return;
        };
        let Some(d) = env.construction.templates[site.template].hunt else {
            return;
        };
        if site.hunt_time.unwrap_or(0) < ticks(d.time * 300.0) {
            return;
        }
        let pos = self.characters[i].position;
        // `FUN_140097730`: the nearest character with a WildAnimalComponent
        // (`DAT_1401d0000`; livestock have none) within 8 of the hunter, the first of equals
        // in the characters' tree's order, which finds nothing while the hunter stands more
        // than 9 from height 0. Whether it is hunted already is asked after.
        let animals = &env.wildlife.animals;
        let best = env
            .wildlife
            .tree
            .nearest(pos, 8.0, |who| match who {
                crate::behaviour::character_tree::Who::Animal(a) => {
                    animals.get(&a).is_some_and(|x| x.livestock.is_none())
                }
                _ => false,
            })
            .and_then(|who| match who {
                crate::behaviour::character_tree::Who::Animal(a) => Some(a),
                _ => None,
            });
        let Some(a) = best else { return };
        let animal = &env.wildlife.animals[&a];
        if animal.flags & crate::nature::wildlife::HUNTED != 0 {
            return;
        }
        let n = env.cells.size as i32;
        let cell_of = |p: [f32; 3]| {
            let half = (n >> 1) as f32;
            let x = ((p[0] + half) as i32).clamp(0, n - 1);
            let y = ((p[1] + half) as i32).clamp(0, n - 1);
            n * y + x
        };
        let (here, at) = (cell_of(pos), cell_of(animal.character.position));
        let Some(nb) = crate::pathfind::nearest_neighbour(
            env.cells.size,
            &env.cells.types,
            at,
            here,
            WALKABLE,
        ) else {
            return;
        };
        let town = TownView {
            size: env.cells.size,
            types: &env.cells.types,
            regions: &self.regions,
            occupants: &env.cells.occupants,
            objects: &env.cells.objects,
            floors: &env.cells.floors,
            entities: &[],
            happiness: &[],
        };
        if !town.reachable(WALKABLE, nb, here) {
            return;
        }
        if let Some((_, done)) = self.characters[i].stack.hunt_watch() {
            *done = true;
        }
        let materials = env
            .wildlife
            .animal_kinds
            .get(&animal.template)
            .map(|k| k.materials.clone())
            .unwrap_or_default();
        let herd = animal.herd;
        let batch = self.states[i].carried.capacity;
        let workplace = self.states[i].workplace;
        let mut duration = ticks(4.0);
        if self.states[i].tool_uses < 1 {
            duration = ticks(crate::scheduler::seconds(duration) * self.rules.no_tool_factor);
        }
        let target = crate::town::floor_ground(&env.cells.floors, env.cells.size, env.terrain, nb);
        let walk = MoveTo::new(target, 1, 0, WALKABLE, 0, -1.0, env.rng);
        let ch = &mut self.characters[i];
        let mut stack = std::mem::take(&mut ch.stack);
        for &m in &materials {
            stack.push(
                ch,
                group,
                Action::CreateRawMaterial {
                    template: m,
                    cell: at,
                    batch,
                    workplace,
                    factor: 1.0,
                },
            );
        }
        stack.push(ch, group, Action::RemoveAnimal { entity: a });
        stack.push(
            ch,
            group,
            Action::PerformWork {
                facing: -1000.0,
                anim: 0xb,
                duration,
                elapsed: 0,
                consume: true,
            },
        );
        stack.push(ch, group, Action::move_to(walk));
        ch.stack = stack;
        // `FUN_14010c730`, then the herd's new target.
        let mut scheduler = crate::scheduler::EntityScheduler::default();
        let mut wild = crate::nature::wildlife::Wild {
            wildlife: env.wildlife,
            placed: env.placed,
            templates: env.templates,
            cells: env.cells,
            terrain: env.terrain,
            rng: env.rng,
            // Taking an animal creates no entity.
            scheduler: &mut scheduler,
        };
        wild.hunted(a);
        if let Some(h) = herd {
            wild.retarget(h);
        }
        if let Some(t) = env
            .construction
            .sites
            .get_mut(&lodge)
            .and_then(|s| s.hunt_time.as_mut())
        {
            *t = 0;
        }
    }
}
