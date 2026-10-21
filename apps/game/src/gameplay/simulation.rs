//! What follows each simulation iteration in the game app: its sounds and terrain changes,
//! decals, particles, weather, achievements and the event log.

use super::*;

impl GameplayLoop {
    /// The month (see `Simulation::month`).
    pub(super) fn month(&self) -> usize {
        self.sim.month()
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

    /// The achievements after the frame's iterations (`Simulation::update_achievements`); one
    /// awarded is kept in the settings store (`Achievement<title>`).
    pub(super) fn update_achievements(&mut self, ctx: &mut Ctx, dt: i64) {
        let won = self.sim.update_achievements(
            ctx.archive,
            &mut crate::ui::strings::UiStrings(&mut ctx.ui.res),
            &self.params,
            dt,
        );
        if let Some(i) = won {
            let title = &self.sim.achievements[i].desc.title;
            ctx.settings
                .persistence
                .set_bool("Achievement", title, true);
            ctx.settings.persistence.write();
        }
    }

    /// The citizens' life events and the sites' events, posted to the event log (see
    /// `Simulation::log_life_events` and `Simulation::log_site_events`).
    pub(super) fn log_events(&mut self, ctx: &mut Ctx) {
        let auto_slow = ctx.settings.game.auto_slow;
        let mut strings = crate::ui::strings::UiStrings(&mut ctx.ui.res);
        self.sim.log_life_events(&mut strings, auto_slow);
        self.sim
            .log_site_events(ctx.archive, &mut strings, auto_slow);
    }
}
