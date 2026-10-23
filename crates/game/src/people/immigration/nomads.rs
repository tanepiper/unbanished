//! Nomads: adding them, walking and wandering, their messages and removing them.

use super::*;

impl Sim<'_, '_> {
    /// `FUN_1401282f0(interact, 2, k)`: create point `k` (the entity's own, then its parts').
    pub(super) fn create_point(&self, e: usize, mut k: usize) -> [f32; 3] {
        for x in self.interact_entities(e) {
            let points = &self.templates[self.placed[x].template].create_points;
            if k < points.len() {
                return self.world_point(x, points[k]);
            }
            k -= points.len();
        }
        self.world_point(e, [0.0; 3])
    }

    /// The nomad entity (`FUN_1400c8bf0`) and its setup (`FUN_140050990`): `Rand(2)` its
    /// variant, placed at `spawn` turned `RandFloatTol × 180`; activation runs state 0 (walk to
    /// `target`).
    pub(super) fn add_nomad(
        &mut self,
        template: usize,
        town_hall: usize,
        spawn: [f32; 3],
        target: [f32; 3],
    ) -> Option<usize> {
        let kind = self.wildlife.nomad_kinds.get(&template)?.clone();
        let variant = self.rng.rand_below(2) as usize;
        let yaw = self.rng.rand_float_tol() * 180.0;
        let id = self.placed.len();
        let t = &self.templates[template];
        self.placed.push(crate::nature::seeding::Placed {
            template,
            bucket: t.bucket,
            cell: (
                self.cell_of(spawn) % self.size as i32,
                self.cell_of(spawn) / self.size as i32,
            ),
            footprint: (1, 1),
            position: spawn,
            yaw,
            variant,
            mesh: usize::MAX,
            growth: 0.0,
            growth_state: None,
            spawn_state: None,
            alive: true,
            removal_requested: false,
            animation: None,
            scale: [1.0; 4],
        });
        let mut character = Character::new(kind.character, spawn, 1.0);
        character.yaw = yaw;
        character.tree_slot = self.wildlife.tree.insert(
            crate::behaviour::character_tree::Who::Nomad(id),
            spawn,
            kind.character.radius,
        );
        self.wildlife.nomads.insert(
            id,
            Nomad {
                template,
                character,
                state: 0,
                town_hall: Some(town_hall),
                spawn,
                target,
                timer: 0,
                variant,
            },
        );
        self.nomad_walk(id, target);
        *self.models_dirty = true;
        Some(id)
    }

    /// `FUN_1400504c0`: the nomad drops what it does and walks to `to`.
    pub(super) fn nomad_walk(&mut self, id: usize, to: [f32; 3]) {
        let Some(nomad) = self.wildlife.nomads.get(&id) else {
            return;
        };
        let Some(group) = self
            .wildlife
            .groups
            .get(&(nomad.template, nomad.variant))
            .cloned()
        else {
            return;
        };
        let walk = MoveTo::new(to, 1, 0, 0xe1f, 0, -1.0, self.rng);
        let ch = &mut self.wildlife.nomads.get_mut(&id).unwrap().character;
        let mut stack = std::mem::take(&mut ch.stack);
        stack.clear(ch, &group, &mut []);
        stack.push(ch, &group, Action::move_to(walk));
        ch.stack = stack;
    }

    pub(super) fn remove_nomad(&mut self, id: usize) {
        if let Some(nomad) = self.wildlife.nomads.remove(&id) {
            self.wildlife.tree.remove(nomad.character.tree_slot);
            self.placed[id].alive = false;
            *self.models_dirty = true;
        }
    }

    /// The nomads' StateMachine updates: 0 at the town hall, it tells it (message 6) and waits; 1
    /// waiting, it wanders now and then; 2 back where it came from, it is gone.
    ///
    /// Original: `FUN_140050320`, `FUN_14004ff60`, `FUN_14004fe90`.
    pub fn update_nomads(&mut self, dt: i64) {
        let ids: Vec<usize> = self.wildlife.nomads.keys().copied().collect();
        for id in ids {
            // The CharacterComponent's scheduled notify runs on the nomad's bucket-1 visit
            // (`Agents::visit_citizens`).
            let Some(nomad) = self.wildlife.nomads.get(&id) else {
                continue;
            };
            if nomad.character.stack.is_active() {
                continue;
            }
            match nomad.state {
                0 => match nomad.town_hall.filter(|t| self.sites.contains_key(t)) {
                    None => self.remove_nomad(id),
                    Some(t) => {
                        self.immigration_message(t, 6);
                        if let Some(n) = self.wildlife.nomads.get_mut(&id) {
                            n.state = 1;
                        }
                    }
                },
                1 => self.nomad_wander(id, dt),
                _ => self.remove_nomad(id),
            }
        }
    }

    /// `FUN_14004ff60`: every 30 ± 15 s, a ground cell (types 0..2) of the 16 × 16 around
    /// where it waits, a third of a cell off its centre.
    pub(super) fn nomad_wander(&mut self, id: usize, dt: i64) {
        let n = self.size as i32;
        let nomad = self.wildlife.nomads.get_mut(&id).unwrap();
        nomad.timer -= dt;
        if 0 <= nomad.timer {
            return;
        }
        let target = nomad.target;
        let t = self.rng.rand_float_tol() * 15.0 + 30.0;
        self.wildlife.nomads.get_mut(&id).unwrap().timer = ticks(t);
        let c = self.cell_of(target);
        let (x0, y0) = (c % n - 8, c / n - 8);
        let mut cells = Vec::new();
        for y in y0..y0 + 16 {
            for x in x0..x0 + 16 {
                if 0 <= x
                    && x < n
                    && 0 <= y
                    && y < n
                    && self.cells.types[(y * n + x) as usize].0 < 3
                {
                    cells.push(y * n + x);
                }
            }
        }
        if cells.is_empty() {
            return;
        }
        let k = self.rng.rand_below(cells.len() as u32) as usize;
        let dy = self.rng.rand_float_tol() * 0.33;
        let dx = self.rng.rand_float_tol() * 0.33;
        let g = self.ground_at(cells[k]);
        self.nomad_walk(id, [g[0] + dx, g[1] + dy, g[2]]);
    }

    /// A nomad's state message: 1 (`FUN_14004fe00`, in 0 and 1) it leaves for where it came
    /// from and forgets the town hall; 0 (`FUN_14004fd80`, in 1) it is gone.
    pub(super) fn nomad_message(&mut self, id: usize, msg: i32) {
        let Some(nomad) = self.wildlife.nomads.get_mut(&id) else {
            return;
        };
        match (nomad.state, msg) {
            (0 | 1, 1) => {
                nomad.state = 2;
                let spawn = nomad.spawn;
                self.nomad_walk(id, spawn);
                if let Some(n) = self.wildlife.nomads.get_mut(&id) {
                    n.town_hall = None;
                }
            }
            (1, 0) => self.remove_nomad(id),
            _ => {}
        }
    }
}
