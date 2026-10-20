//! What follows each simulation iteration in the game app: its sounds and terrain changes,
//! decals, particles, weather, achievements and the event log.

use super::*;

impl GameplayLoop {
    /// The month (see `Simulation::month`).
    pub(super) fn month(&self) -> usize {
        self.sim.month()
    }

    /// Applies the town's queued player commands and placed sites (see
    /// `Simulation::apply_commands`).
    pub(super) fn apply_commands(&mut self) {
        self.sim.apply_commands();
    }

    /// One simulation iteration (`Simulation::iterate`), then what it changed for drawing and
    /// sound: its 3D effects wait for the next audio update, and the scene view's terrain
    /// follows new heights. Returns whether spawning or removal changed the model instances.
    pub(super) fn iterate(&mut self, archive: &unbanished_formats::pkg::Archive, dt: f32) -> bool {
        let step = self.sim.iterate(archive, dt);
        for (p, effect) in step.sounds {
            self.pending_sounds.push((glam::Vec3::from(p), effect));
        }
        if step.reshaped {
            if let (Some(view), Some(heights)) = (&mut self.view, &self.sim.heights) {
                view.terrain.heights.clone_from(&heights.values);
                view.terrain.reshaped();
            }
        }
        step.models_changed
    }

    /// The map's decals when what they are built from changed: the entities' (`+0x1f0`,
    /// `FUN_1400b2310`), then the roads' (`FUN_1400b4840`).
    pub(super) fn refresh_decals(&mut self, archive: &unbanished_formats::pkg::Archive) {
        let (Some(town), Some(view)) = (self.sim.town.as_ref(), self.view.as_mut()) else {
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
            .sim
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
        let (Some(town), Some(view)) = (self.sim.town.as_ref(), self.view.as_mut()) else {
            return;
        };
        let grid = self
            .sim
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
        let (Some(view), Some(env)) = (self.view.as_mut(), self.sim.environment.as_ref()) else {
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
        let Some(town) = &self.sim.town else { return };
        // The TerrainGenerator's resource is named for the terrain.
        let terrain = format!(r"Game\Terrain\{}.rsc", self.params.terrain);
        let x = crate::achievements::GoalContext {
            town,
            statistics: &self.sim.statistics,
            year: self.sim.calendar.year,
            year_seconds: self.sim.calendar.seconds(),
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
            let (month, year) = (self.month(), self.sim.calendar.year as i32);
            self.sim.events.add(
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
        let (month, year) = (self.month(), self.sim.calendar.year as i32);
        let Some(town) = &mut self.sim.town else {
            return;
        };
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
            self.sim.events.add(
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
        let (month, year) = (self.month(), self.sim.calendar.year as i32);
        let Some(town) = &mut self.sim.town else {
            return;
        };
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
            self.sim.events.add(
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
