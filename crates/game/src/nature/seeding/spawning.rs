//! Spawning: where a child may grow, spawning one from a parent, planting, a template's spawn state, periodic and scheduled spawns, and removing an entity.

use super::*;

impl Seeder<'_> {
    /// `FUN_1400a4aa0(map, x, y, 4, 0.98, 1000, 0, …)`: buildable ground with no entity and a
    /// flat enough surface.
    pub(super) fn spawn_spot_ok(&self, x: i32, y: i32) -> bool {
        let n = self.n();
        if x < 0 || y < 0 || x >= n || y >= n {
            return false;
        }
        let i = y * n + x;
        if self.cells.types[i as usize] != CellType::GROUND {
            return false;
        }
        if self.cells.occupants[i as usize].is_some() || self.cells.flags[i as usize] != 0 {
            return false;
        }
        let (px, py) = self.cell_position(i);
        let mut normal = [0.0f32; 3];
        let h = self
            .cells
            .floors
            .ground_normal(self.cells.size, self.terrain, px, py, &mut normal);
        SPAWN_NORMAL <= normal[2] && (h - 0.0).abs() <= SPAWN_HEIGHT_TOLERANCE
    }

    /// `FUN_140056620`: spawn children around a newly grown parent.
    pub(super) fn spawn(&mut self, parent: usize) {
        let template = self.placed[parent].template;
        let growth = self.placed[parent].growth;
        for (entry, (child, lo, hi)) in self.templates[template]
            .spawn
            .clone()
            .into_iter()
            .enumerate()
        {
            if !(lo <= growth && growth <= hi) {
                continue;
            }
            let step = self.templates[child]
                .growth
                .as_ref()
                .map_or(1.0, |g| g.max_base);
            let attempts = (growth / step) as i32 + 1;
            for _ in 0..attempts {
                self.spawn_child(parent, entry, true, false);
            }
        }
    }

    /// Startup uses random_growth=true; scheduled spawning uses false. The child is registered in
    /// the PARENT TEMPLATE's bucket (not its own template's bucket).
    ///
    /// Original: `FUN_1400560e0`.
    pub fn spawn_child(
        &mut self,
        parent: usize,
        entry: usize,
        random_growth: bool,
        disabled: bool,
    ) -> Option<usize> {
        if disabled {
            return None;
        }
        let parent_template = self.placed[parent].template;
        let child = self.templates[parent_template].spawn[entry].0;
        let (px, py) = self.placed[parent].cell;
        let g = self.templates[child].growth.clone()?;
        if self.rng.rand_float() > ONE / g.spawn_one_in as f32 {
            return None;
        }
        let x = px + self.rng.rand_tol(g.spawn_radius);
        let y = py + self.rng.rand_tol(g.spawn_radius);
        let n = self.n();
        if x < 0 || y < 0 || x >= n || y >= n || !self.spawn_spot_ok(x, y) {
            return None;
        }
        if !self.density_ok(child, x, y) {
            return None;
        }
        let (variant, mesh, max_growth) = self.construct(child);
        let (position, yaw) = self.position(child, n * y + x);
        let (state, growth, scale) = self.set_growth(child, max_growth, 0.0);
        let id = self.register(Placed {
            template: child,
            bucket: self.templates[parent_template].bucket,
            cell: (x, y),
            footprint: (1, 1),
            position,
            yaw,
            variant,
            mesh,
            growth,
            growth_state: Some(state),
            spawn_state: self.spawn_state(child),
            alive: true,
            removal_requested: false,
            animation: None,
            scale,
        });
        if random_growth {
            let fraction = self.rng.rand_float();
            let (state, growth, scale) = self.set_growth(child, max_growth, fraction);
            self.placed[id].growth = growth;
            self.placed[id].scale = scale;
            self.placed[id].growth_state = Some(state);
        }
        Some(id)
    }

    /// CreateNaturalResource's start on a free cell: clone the template, place it in the cell,
    /// growth 0, then a random model variant and mesh, and register it.
    ///
    /// Original: `FUN_1400cbb20`, `FUN_1400c8bf0`, `FUN_14012ba10`, `FUN_140126280`,
    /// `FUN_14012a8a0`.
    pub fn plant(&mut self, template: usize, cell: i32) -> usize {
        let (_, _, max_growth) = self.construct(template);
        let (position, yaw) = self.position(template, cell);
        let (state, growth, scale) = self.set_growth(template, max_growth, 0.0);
        let variants = self.templates[template]
            .model
            .as_ref()
            .map_or(1, |m| m.variants.len().max(1));
        let variant = self.rng.rand_below(variants as u32) as usize;
        let meshes = self.templates[template]
            .model
            .as_ref()
            .and_then(|m| m.variants.get(variant))
            .map_or(1, |v| v.meshes.len().max(1));
        let mesh = self.rng.rand_below(meshes as u32) as usize;
        let n = self.n();
        self.register(Placed {
            template,
            bucket: self.templates[template].bucket,
            cell: (cell % n, cell / n),
            footprint: (1, 1),
            position,
            yaw,
            variant,
            mesh,
            growth,
            growth_state: Some(state),
            spawn_state: self.spawn_state(template),
            alive: true,
            removal_requested: false,
            animation: None,
            scale,
        })
    }

    pub(super) fn spawn_state(&self, template: usize) -> Option<Spawn> {
        self.templates[template]
            .updates
            .contains(&ScheduledComponent::Spawn)
            .then(|| Spawn::new(self.templates[template].spawn.len()))
    }

    /// Port of FUN_140125070: reproduce using the parent's template and model variant. Unlike
    /// FUN_1400560e0's startup children, this path initializes the child's growth at zero and
    /// selects a mesh after the timer draw; it never draws a random initial growth fraction.
    pub(super) fn periodic_spawn(&mut self, parent: usize) -> Option<usize> {
        let template = self.placed[parent].template;
        let g = self.templates[template].growth.as_ref()?;
        if self.rng.rand_float() > ONE / g.spawn_one_in as f32 {
            return None;
        }
        let (px, py) = self.placed[parent].cell;
        let x = px + self.rng.rand_tol(g.spawn_radius);
        let y = py + self.rng.rand_tol(g.spawn_radius);
        if !self.spawn_spot_ok(x, y) || !self.density_ok(template, x, y) {
            return None;
        }
        let variant = self.placed[parent].variant;
        let (_, _, maximum) = self.construct(template);
        let (position, yaw) = self.position(template, y * self.n() + x);
        let (state, growth, scale) = self.set_growth(template, maximum, 0.0);
        let meshes = self.templates[template]
            .model
            .as_ref()?
            .variants
            .get(variant)?
            .meshes
            .len();
        let mesh = self.rng.rand_below(meshes as u32) as usize;
        Some(self.register(Placed {
            template,
            bucket: self.templates[template].bucket,
            cell: (x, y),
            footprint: (1, 1),
            position,
            yaw,
            variant,
            mesh,
            growth,
            growth_state: Some(state),
            spawn_state: self.spawn_state(template),
            alive: true,
            removal_requested: false,
            animation: None,
            scale,
        }))
    }

    /// SpawnComponent's scheduled update (`FUN_140056430`) for one visited entity: each entry
    /// whose growth band holds the parent's growth runs its clock, and every child growth
    /// period (`+0x28 × 300`) it tries a child (`FUN_1400560e0`, without random growth).
    pub(super) fn scheduled_spawn(
        &mut self,
        scheduler: &mut EntityScheduler<usize>,
        id: usize,
        elapsed: i64,
        disabled: bool,
    ) -> bool {
        let p = &self.placed[id];
        if !p.alive || p.spawn_state.is_none() {
            return false;
        }
        let growth = p.growth;
        let entries: Vec<(f32, f32, f32)> = self.templates[p.template]
            .spawn
            .iter()
            .map(|&(child, lo, hi)| {
                let period = self.templates[child].growth.as_ref().map_or(0.0, |g| g.f28);
                (lo, hi, period)
            })
            .collect();
        let attempts = self.placed[id]
            .spawn_state
            .as_mut()
            .unwrap()
            .update(growth, elapsed, &entries);
        let mut changed = false;
        for (entry, count) in attempts {
            for _ in 0..count {
                if let Some(child) = self.spawn_child(id, entry, false, disabled) {
                    let handle = scheduler.register_at(child, child, self.placed[child].bucket);
                    debug_assert_eq!(handle, child);
                    changed = true;
                }
            }
        }
        changed
    }

    /// Port of FUN_140125a30 and FUN_1400a33b0 for one-cell natural-resource removal. Keep
    /// handles stable; the scheduler swaps its bucket entries separately at the end of the tick.
    pub(super) fn remove(&mut self, id: usize) {
        let p = &mut self.placed[id];
        if !p.alive {
            return;
        }
        p.alive = false;
        self.removed.push(id);
        let (x, y) = p.cell;
        let i = y as usize * self.cells.size + x as usize;
        if let Some(d) = &self.templates[p.template].growth {
            if p.growth != 0.0 {
                let factor = if self.cells.previous_template[i] == Some(p.template) {
                    d.f3c
                } else {
                    d.f40
                };
                self.cells.growth_bytes[i] =
                    adjust_map_byte(self.cells.growth_bytes[i], p.growth.min(1.0) * factor);
                self.cells.previous_template[i] = Some(p.template);
            }
        }
        if self.cells.objects[i] == Some(id) {
            let ty = self.terrain.natural_cell_type(self.cells.size, i);
            self.cells.set_type(i, ty);
            self.cells.objects[i] = None;
        }
    }
}
