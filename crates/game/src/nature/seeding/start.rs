//! A new map's start: the start structures, the first seeding of trees, rocks and ore from Perlin noise, and registering what is placed.

use super::*;

impl Seeder<'_> {
    // Game +0xd0 |= 1
    /// The profile's start structures, relative to the start cell.
    ///
    /// Each is built in "instant" mode (`Game |= 1`), so the build component shows the last
    /// (finished) mesh. Component setup draws nothing from the stream except the model's variant;
    /// buildings do not stamp the map on activation.
    ///
    /// Original: `FUN_1400a7490`, `FUN_14010cfb0`.
    pub fn place_structures(
        &mut self,
        structures: &[(PlacedStructure, Option<usize>)],
        sx: i32,
        sy: i32,
    ) {
        let n = self.n();
        let corner = |s: &Self, x: i32, y: i32| {
            let i = n * y + x;
            let half = (n >> 1) as f32;
            let (px, py) = ((i % n) as f32 - half, (i / n) as f32 - half);
            let (z, _) = s.ground(px, py);
            [px, py, z]
        };
        for (record, template) in structures {
            let Some(t) = *template else { continue };
            // Size from the template, otherwise the record's two integers; swapped when turned
            // by ±90 or 270 degrees.
            let (mut w, mut h) = self.templates[t].size.unwrap_or((record.a, record.b));
            if matches!(record.rotation, 90 | -90 | 270) {
                std::mem::swap(&mut w, &mut h);
            }
            let (x, y) = (sx + record.x, sy + record.y);
            let p0 = corner(self, x, y);
            let p1 = corner(self, x + w, y + h);
            let position = [
                (p1[0] + p0[0]) * HALF,
                (p1[1] + p0[1]) * HALF,
                (p1[2] + p0[2]) * HALF,
            ];
            self.construct(t);
            // The yard is an entity with storage/map components but no model. Retain it;
            // only model-bearing structures consume the model-variant draw.
            let (variant, mesh) = self.templates[t].model.as_ref().map_or((0, 0), |model| {
                let variant = self.rng.rand_below(model.variants.len() as u32) as usize;
                let mesh = model
                    .variants
                    .get(variant)
                    .map_or(0, |v| v.meshes.len().saturating_sub(1));
                (variant, mesh)
            });
            self.placed.push(Placed {
                template: t,
                bucket: self.templates[t].bucket,
                cell: (x, y),
                footprint: (w, h),
                position,
                yaw: record.rotation.rem_euclid(360) as f32,
                variant,
                mesh,
                growth: 0.0,
                growth_state: None,
                spawn_state: None,
                alive: true,
                removal_requested: false,
                animation: None,
                scale: [ONE; 4],
            });
        }
    }

    /// Seed every template of the profile's list over the map, away from the start `(sx, sy)` by
    /// more than `sqrt(radius_sq)` cells.
    ///
    /// Original: `FUN_1400a6780`.
    pub fn seed(&mut self, list: &[usize], sx: i32, sy: i32, radius_sq: i32) {
        // Three fields the pass builds and never reads.
        for _ in 0..3 {
            self.perlin(ONE, ONE);
        }
        let n = self.n();
        for &template in list {
            let Some(nr) = self.templates[template].natural.clone() else {
                continue;
            };
            let variants = self.templates[template]
                .model
                .as_ref()
                .map_or(0, |m| m.variants.len());
            let clusters: Vec<PerlinNoise> = if nr.cluster {
                (0..variants)
                    .map(|_| self.perlin(nr.cluster_persistence, nr.cluster_octaves))
                    .collect()
            } else {
                Vec::new()
            };
            let field = self.perlin(nr.noise_persistence, nr.noise_octaves);
            let inv = ONE / nr.noise_scale;
            for y in 0..n {
                for x in 0..n {
                    let v = field.generate(x as f32 * inv, y as f32 * inv);
                    if nr.noise_min > v * HALF + HALF {
                        continue;
                    }
                    if self.rng.rand_float() > nr.chance {
                        continue;
                    }
                    let index = n * y + x;
                    if self.cells.objects[index as usize].is_some()
                        || (y - sy) * (y - sy) + (x - sx) * (x - sx) < radius_sq
                    {
                        continue;
                    }
                    let (cx, cy) = self.cell_position(index);
                    let (h, up) = self.ground(cx, cy);
                    if h < nr.height_min || (nr.height_max <= h && h != nr.height_max) {
                        continue;
                    }
                    if up < nr.normal_min {
                        continue;
                    }
                    if !self.density_ok(template, x, y) {
                        continue;
                    }
                    let (mut variant, mut mesh, max_growth) = self.construct(template);
                    let (position, yaw) = self.position(template, index);
                    let (mut growth, mut scale, mut growth_state) = (1.0, [ONE; 4], None);
                    if self.templates[template].growth.is_some() {
                        let fraction = self.rng.rand_float();
                        let (state, value, output) =
                            self.set_growth(template, max_growth, fraction);
                        growth = value;
                        scale = output;
                        growth_state = Some(state);
                    }
                    let id = self.register(Placed {
                        template,
                        bucket: self.templates[template].bucket,
                        cell: (x, y),
                        footprint: (1, 1),
                        position,
                        yaw,
                        variant,
                        mesh,
                        growth,
                        growth_state,
                        spawn_state: self.spawn_state(template),
                        alive: true,
                        removal_requested: false,
                        animation: None,
                        scale,
                    });
                    if !self.templates[template].spawn.is_empty() {
                        self.spawn(id);
                    }
                    if nr.cluster {
                        let inv = ONE / nr.cluster_scale;
                        variant = 0;
                        for (k, c) in clusters.iter().enumerate() {
                            let v = c.generate(x as f32 * inv, y as f32 * inv);
                            // jbe at 0x1400a7221: at the threshold or above, or NaN, this one.
                            #[allow(clippy::neg_cmp_op_on_partial_ord)]
                            if !(v * HALF + HALF < nr.cluster_threshold) {
                                variant = k;
                                break;
                            }
                        }
                        if self.rng.rand_below(nr.random_variant_one_in as u32) == 0 {
                            variant = self.rng.rand_below(variants as u32) as usize;
                        }
                        let meshes = self.templates[template].model.as_ref().unwrap().variants
                            [variant]
                            .meshes
                            .len();
                        mesh = self.rng.rand_below(meshes as u32) as usize;
                        self.placed[id].variant = variant;
                        self.placed[id].mesh = mesh;
                    }
                }
            }
        }
    }

    pub(super) fn perlin(&mut self, persistence: f32, octaves: f32) -> PerlinNoise {
        let a = self.rng.rand_tol(0x7ff_ffff);
        let b = self.rng.rand_tol(0x7ff_ffff);
        PerlinNoise::new(persistence, octaves, b, a)
    }

    /// `FUN_1400c8bf0` setup pass for a natural resource: the model component picks a variant
    /// (`FUN_140129f90`) and the growth component its maximum (`FUN_140125bc0`), in template
    /// order (map, model, naturalresource, growth, spawn).
    pub(super) fn construct(&mut self, template: usize) -> (usize, usize, f32) {
        let t = &self.templates[template];
        let (mut variant, mut mesh) = (0usize, 0usize);
        if let Some(m) = &t.model {
            variant = m.variant.max(0) as usize;
            if m.random_variant {
                variant = self.rng.rand_below(m.variants.len() as u32) as usize;
            }
            mesh = m.mesh.max(0) as usize;
        }
        let mut max_growth = 1.0;
        if let Some(g) = &t.growth {
            max_growth = if g.max_base <= 0.0 {
                -1.0
            } else {
                self.rng.rand_float_tol() * g.max_range + g.max_base
            };
        }
        (variant, mesh, max_growth)
    }

    /// `FUN_14012ba10`: jitter the entity inside its cell and pick a yaw.
    pub(super) fn position(&mut self, template: usize, index: i32) -> ([f32; 3], f32) {
        let scale = self.templates[template]
            .natural
            .as_ref()
            .map_or(0.0, |n| n.position_scale);
        let (x, y) = self.cell_position(index);
        let (z, _) = self.ground(x, y);
        let dy = self.rng.rand_float_tol();
        let dx = self.rng.rand_float_tol();
        let position = [dx * scale + x, dy * scale + y, z + 0.0];
        let yaw = self.rng.rand_float_tol() * YAW_RANGE;
        (position, yaw)
    }

    /// `FUN_140126280`: set the growth from a 0..1 fraction of the maximum and draw the timer.
    pub(super) fn set_growth(
        &mut self,
        template: usize,
        max_growth: f32,
        fraction: f32,
    ) -> (Growth, f32, [f32; 4]) {
        Growth::initialise(
            self.templates[template].growth.as_ref().unwrap(),
            max_growth,
            fraction,
            self.rng,
        )
    }

    /// `FUN_1400c9d70` → `FUN_140129840`: register and stamp the map footprint (natural
    /// resources: one cell of the description's type).
    pub(super) fn register(&mut self, placed: Placed) -> usize {
        let id = self.placed.len();
        if let Some(map) = self.templates[placed.template].map.clone() {
            if map.stamp && !map.no_stamp {
                let i = (placed.cell.1 * self.n() + placed.cell.0) as usize;
                // FUN_1400a30f0 queues the old stamped entity's removal before replacing it.
                if let Some(old) = self.cells.objects[i] {
                    self.remove(old);
                }
                self.cells.set_type(i, CellType(map.cell_type as u8));
                self.cells.objects[i] = Some(id);
            }
        }
        self.placed.push(placed);
        id
    }

    /// `FUN_140126100`: at most `max_neighbours` occupied cells and `max_same` of the same
    /// growth type among the 8 neighbours. Cached external-reference identities replace the
    /// original GrowthDescription pointer comparison (including descriptions shared by templates).
    pub(crate) fn density_ok(&self, template: usize, x: i32, y: i32) -> bool {
        let Some(growth) = &self.templates[template].growth else {
            return true;
        };
        let n = self.n();
        let (mut occupied, mut same) = (0, 0);
        for cy in y - 1..y + 2 {
            for cx in x - 1..x + 2 {
                if (cx == x && cy == y) || cx < 0 || cy < 0 || cx >= n || cy >= n {
                    continue;
                }
                if let Some(o) = self.cells.objects[(cy * n + cx) as usize] {
                    occupied += 1;
                    if self.templates[self.placed[o].template].growth_reference
                        == self.templates[template].growth_reference
                    {
                        same += 1;
                    }
                }
            }
        }
        !(growth.max_same < same || growth.max_neighbours < occupied)
    }
}
