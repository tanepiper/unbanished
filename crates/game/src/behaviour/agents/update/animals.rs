//! Moving the animals and telling them about hunters.

use super::*;

impl Agents {
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
    pub(in crate::behaviour::agents) fn hunt_notify(
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
