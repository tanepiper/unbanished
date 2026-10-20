//! One simulation iteration and what follows it: commands, the town's update, decals, particles, weather, achievements and the event log.

use super::*;

impl GameplayLoop {
    /// `floor(elapsed / 3600 × 12)`.
    pub(super) fn month(&self) -> usize {
        ((self.calendar.seconds() / YEAR_SECONDS * MONTHS_PER_YEAR as f32) as usize)
            .min(MONTHS_PER_YEAR - 1)
    }

    /// Applies the town's queued player commands and placed sites, with the calendar and
    /// weather they read.
    pub(super) fn apply_commands(&mut self) {
        let (Some(town), Some(heights)) = (&mut self.town, &self.heights) else {
            return;
        };
        let grid = heights.grid();
        // The crop fields read the calendar (`Game +0x128`).
        town.construction.calendar = self.calendar.seconds();
        town.construction.year = self.calendar.year;
        // Fire spreads less in rain (environment `+0x10`).
        town.construction.precipitation = self.environment.as_ref().is_some_and(|e| e.state == 0);
        town.apply_commands(&grid, &mut self.rng);
    }

    /// Port of FUN_14009b850: entity scheduler, integer calendar, then environment.
    /// Growth reads the prior environment temperature; weather draws follow entity callbacks.
    /// Returns whether spawning/removal changed the model instances.
    pub(super) fn iterate(&mut self, archive: &unbanished_formats::pkg::Archive, dt: f32) -> bool {
        let mut models_changed = false;
        // Player commands and placed sites take effect between iterations.
        self.apply_commands();
        if let (Some(town), Some(heights)) = (&mut self.town, &self.heights) {
            let grid = heights.grid();
            let temperature = self.environment.as_ref().map_or(0.0, |e| e.temperature);
            let changed = {
                let _p = crate::platform::perf::scope("sim.town");
                town.iterate(&grid, &mut self.rng, dt, temperature)
            };
            // AnimationManager::Update runs inside the same iteration (FUN_14009b850).
            let stock = std::array::from_fn(|i| {
                (self.stock_warnings.count[i], self.stock_warnings.limits[i])
            });
            let changed = {
                let _p = crate::platform::perf::scope("sim.citizens");
                town.update_citizens(&grid, &mut self.rng, dt, temperature, stock)
            } | changed;
            // AnimAudioEvents that passed their draw: `PlayEffect3D(effect, 4)` at the entity.
            for (entity, effect) in town.animation_events.drain(..) {
                let p = town.placed[entity].position;
                self.pending_sounds
                    .push((glam::Vec3::new(p[0], p[1], p[2]), effect));
            }
            models_changed = changed;
        }
        // The tunnels' terraforming and the flattening under stamped footprints and roads, on
        // the terrain the town reads (`FUN_140057c70`, `FUN_1400a35b0`).
        if let (Some(town), Some(heights)) = (&mut self.town, &mut self.heights) {
            let c = &town.construction;
            if !c.terraform_requests.is_empty() || !c.flatten_requests.is_empty() {
                let width = heights.width();
                if town.apply_terraforms(&mut heights.values, width) {
                    // The scene view draws the new heights: its mesh follows.
                    if let Some(view) = &mut self.view {
                        view.terrain.heights.clone_from(&heights.values);
                        view.terrain.reshaped();
                    }
                }
            }
        }
        self.calendar.advance(ticks(dt));
        let mut tornado = false;
        if let Some(env) = &mut self.environment {
            tornado = env.update(self.calendar.seconds(), dt, &mut self.rng);
        }
        // `FUN_1400999b0` step 6: the environment template's entity.
        if tornado {
            if let (Some(town), Some(heights), Some(reference)) = (
                &mut self.town,
                &self.heights,
                self.environment_template.as_deref(),
            ) {
                let grid = heights.grid();
                town.spawn_tornado(archive, reference, &grid, &mut self.rng);
                models_changed = true;
            }
        }
        // `FUN_140091780`: recompute stock (`FUN_14008f3a0`), then its warnings
        // (`FUN_1400915d0`, posted through `FUN_14009aa50` at severity 3).
        if let Some(town) = &mut self.town {
            self.stock_warnings
                .recompute(&town.registered_storages(), &town.materials);
            // StockManager::Update's commute optimisation (every 300 s).
            if let Some(heights) = &self.heights {
                let grid = heights.grid();
                town.update_commute(&grid, &mut self.rng, dt);
            }
            // The job manager's pass and clock (`FUN_14009b850`, after the stock update).
            town.construction.jobs.step(ticks(dt));
            // `FUN_14008fc00` every 75 s: the citizens and every stock slot.
            let life = &town.agents.life;
            let samples: Vec<unbanished_game::work::tracker::CitizenSample> = town
                .citizens
                .iter()
                .zip(&town.agents.states)
                .filter(|(c, _)| c.alive)
                .map(|(c, s)| unbanished_game::work::tracker::CitizenSample {
                    adult: life.adult_age() <= c.age,
                    school: s.school.is_some(),
                    health: s.health,
                    happiness: s.happiness,
                    educated: life.i(0x118) <= c.education,
                    coat: s.clothing != 0,
                })
                .collect();
            let (health_max, happiness_max) = (life.f(0x70), life.f(0xa4));
            self.statistics.update(
                ticks(dt),
                &samples,
                health_max,
                happiness_max,
                &self.stock_warnings.count,
                &mut town.construction.counters,
            );
            let (month, year) = (self.month(), self.calendar.year as i32);
            for kind in self.stock_warnings.check(ticks(dt)) {
                // AutoSlow is applied once the frame's events are in (`slow_request`).
                self.events
                    .add(kind, 3, -1, month, year, Default::default(), true);
            }
        }
        models_changed
    }

    /// The map's decals when what they are built from changed: the entities' (`+0x1f0`,
    /// `FUN_1400b2310`), then the roads' (`FUN_1400b4840`).
    pub(super) fn refresh_decals(&mut self, archive: &unbanished_formats::pkg::Archive) {
        let (Some(town), Some(view)) = (self.town.as_ref(), self.view.as_mut()) else {
            return;
        };
        // Templates are added as they are first placed.
        let known = self.decal_descriptions.len();
        if known < town.templates.len() {
            self.decal_descriptions
                .extend(crate::gfx::building_decals::descriptions(
                    archive,
                    &town.templates[known..],
                ));
        }
        let decals = &town.construction.decals;
        // Work radii (`FUN_140055d90`): the placement ghost's, then each open window's with a
        // RadiusUI about its building's centre (`FUN_14007cb20`).
        let mut rings: Vec<((i32, i32), i32, String)> = self
            .placement
            .as_ref()
            .and_then(|t| t.radius_decal())
            .map(|(cell, r, m)| (cell, r, m.to_string()))
            .into_iter()
            .collect();
        for e in self.entities.radius_windows() {
            let Some(p) = town.placed.get(e).filter(|p| p.alive) else {
                continue;
            };
            let template = p.template;
            let radius = self.radius_descriptions.entry(template).or_insert_with(|| {
                let file = town.templates[template]
                    .reference
                    .split(':')
                    .next()?
                    .replace('\\', "/");
                let data = archive.read_ref(&format!("{file}:radius")).ok()?;
                let (r, m) = unbanished_formats::game::radius_description(&data).ok()?;
                Some((r, m?))
            });
            if let Some((r, m)) = radius {
                let cell = (
                    p.cell.0 + (p.footprint.0 >> 1),
                    p.cell.1 + (p.footprint.1 >> 1),
                );
                rings.push((cell, *r, m.clone()));
            }
        }
        let key = (
            town.cells.road_revision,
            view.terrain.revision,
            crate::gfx::building_decals::signature(&town.placed, &self.decal_descriptions, decals),
            rings.iter().map(|(c, r, _)| (*c, *r)).collect::<Vec<_>>(),
        );
        if self.decals_built.as_ref() == Some(&key) {
            return;
        }
        self.decals_built = Some(key);
        let grid = self
            .heights
            .as_ref()
            .expect("the heights come with the view")
            .grid();
        let mut list = crate::gfx::building_decals::build(
            archive,
            &town.placed,
            &self.decal_descriptions,
            decals,
            &grid,
            town.cells.size,
            &mut self.decal_cache,
        );
        for (cell, r, material) in &rings {
            list.extend(crate::gfx::building_decals::radius(
                archive,
                *cell,
                *r,
                material,
                &grid,
                town.cells.size,
                &mut self.decal_cache,
            ));
        }
        let entities = list.len();
        list.extend(crate::gfx::road_decals::build(
            archive,
            &town.cells,
            &grid,
            &self.road_materials,
            &mut self.decal_cache,
        ));
        log::debug!("decals rebuilt: {entities} entities, {} in all", list.len());
        if let Some(e) = &mut view.extras {
            e.decals = list;
            view.decals_revision += 1;
        }
    }

    /// The ParticleComponents' slots against the town, their systems run by the game time
    /// (`FUN_140053490`, the manager's delta), and this frame's draws.
    pub(super) fn refresh_particles(
        &mut self,
        archive: &unbanished_formats::pkg::Archive,
        dt: f32,
    ) {
        let (Some(town), Some(view)) = (self.town.as_ref(), self.view.as_mut()) else {
            return;
        };
        let grid = self
            .heights
            .as_ref()
            .expect("the heights come with the view")
            .grid();
        self.particles.drive(archive, town, &grid);
        if 0.0 < dt {
            self.particles.update(dt);
        }
        view.particles = self.particles.batches(archive);
        for b in &view.particles {
            if !view.particle_textures.contains_key(&b.material) {
                let textures = crate::view::scene_assets::particle_textures(archive, &b.material);
                view.particle_textures.insert(b.material.clone(), textures);
            }
        }
    }

    /// The rain and snow (`WeatherGeometry::Update` by the game time): each slot's particles at
    /// its effect's intensity, none with ShowWeather off (`FUN_1400999b0`).
    pub(super) fn refresh_weather(&mut self, ctx: &mut Ctx, dt: f32) {
        let show = ctx.settings.game.show_weather;
        let (Some(view), Some(env)) = (self.view.as_mut(), self.environment.as_ref()) else {
            return;
        };
        view.weather.clear();
        for (slot, p) in self.precipitation.iter_mut().enumerate() {
            let Some(p) = p else { continue };
            let intensity = env
                .effects
                .get(slot)
                .copied()
                .flatten()
                .filter(|_| show)
                .map_or(0.0, |e| e.intensity);
            if intensity <= 0.0 {
                continue;
            }
            if 0.0 < dt {
                p.update(dt);
            }
            if let Some(b) = p.batch(intensity) {
                if !view.weather_textures.contains_key(&b.material) {
                    let t =
                        crate::view::scene_assets::particle_textures(ctx.archive, &b.material).0;
                    view.weather_textures.insert(b.material.clone(), t);
                }
                view.weather.push(b);
            }
        }
    }

    /// `AchievementManager::Update(dt × iterations)` after the frame's iterations; one awarded
    /// is stored and logged (event `0x19`, `FUN_14009be30`, `@2` its title). Nothing is
    /// checked, nor any time gathered, while user pack files are enabled (`FileSystem +0x38`).
    pub(super) fn update_achievements(&mut self, ctx: &mut Ctx, dt: i64) {
        if 0 < ctx.archive.user_packs {
            return;
        }
        let Some(town) = &self.town else { return };
        // The TerrainGenerator's resource is named for the terrain.
        let terrain = format!(r"Game\Terrain\{}.rsc", self.params.terrain);
        let x = crate::achievements::GoalContext {
            town,
            statistics: &self.statistics,
            year: self.calendar.year,
            year_seconds: self.calendar.seconds(),
            terrain: &terrain,
            climate: &self.params.climate_ref,
            size: self.params.size as i32,
        };
        let won = self
            .achievement_manager
            .update(&self.achievements, dt, &x, Some(ctx.archive));
        if let Some(i) = won {
            let a = &self.achievements[i].desc;
            let title = ctx.ui.res.string(a.strings.as_deref(), &a.title);
            ctx.settings
                .persistence
                .set_bool("Achievement", &a.title, true);
            ctx.settings.persistence.write();
            let (month, year) = (self.month(), self.calendar.year as i32);
            self.events.add(
                0x19,
                4,
                -1,
                month,
                year,
                [title, String::new(), String::new()],
                false,
            );
        }
    }

    /// `FUN_140118fc0`: the citizens' births, deaths and coming of age posted to the event
    /// log with `@2` the citizen's name, `@3` its role (profession, child or student) and `@4`
    /// the disease's or accident's name.
    pub(super) fn log_life_events(&mut self, ctx: &mut Ctx) {
        let (month, year) = (self.month(), self.calendar.year as i32);
        let Some(town) = &mut self.town else { return };
        let events = std::mem::take(&mut town.agents.events);
        if events.is_empty() {
            return;
        }
        for e in events {
            let Some(c) = town.citizens.get(e.citizen) else {
                continue;
            };
            let state = &town.agents.states[e.citizen];
            let child = c.age < town.rules.adult_age;
            let key = if c.school.is_some() {
                Some("student")
            } else if child {
                Some("child")
            } else {
                None
            };
            let role = match key {
                Some(k) => town.profession(k).map(|p| (p.string_table, p.name)),
                None => town
                    .construction
                    .professions
                    .list
                    .get(state.profession)
                    .map(|p| (p.string_table.clone(), p.name.clone())),
            }
            .map(|(t, k)| ctx.ui.res.string(t.as_deref(), &k))
            .unwrap_or_default();
            let extra = e
                .extra
                .map(|(t, k)| ctx.ui.res.string(t.as_deref(), &k))
                .unwrap_or_default();
            let auto_slow = ctx.settings.game.auto_slow;
            self.events.add(
                e.kind,
                e.severity,
                -1,
                month,
                year,
                [c.name.clone(), role, extra],
                auto_slow,
            );
        }
    }

    /// The sites' events (`FUN_14009a2d0` with the entity): `@2` its toolbar name, at its
    /// map-rectangle centre cell (`FUN_14009a500`).
    pub(super) fn log_site_events(&mut self, ctx: &mut Ctx) {
        let (month, year) = (self.month(), self.calendar.year as i32);
        let Some(town) = &mut self.town else { return };
        let events = std::mem::take(&mut town.construction.site_events);
        for (kind, severity, e) in events {
            let Some(p) = town.placed.get(e) else {
                continue;
            };
            let reference = &town.templates[p.template].reference;
            let file = reference.split(':').next().unwrap_or("");
            let name = ctx
                .archive
                .read_ref(&format!("{file}:toolbar"))
                .ok()
                .and_then(|d| unbanished_formats::game::toolbar_description(&d).ok())
                .map(|t| ctx.ui.res.string(t.string_table.as_deref(), &t.name))
                .unwrap_or_default();
            let n = town.cells.size as i32;
            let cell = (p.cell.1 + (p.footprint.1 >> 1)) * n + p.cell.0 + (p.footprint.0 >> 1);
            let auto_slow = ctx.settings.game.auto_slow;
            self.events.add(
                kind,
                severity,
                cell,
                month,
                year,
                [name, String::new(), String::new()],
                auto_slow,
            );
        }
    }
}
