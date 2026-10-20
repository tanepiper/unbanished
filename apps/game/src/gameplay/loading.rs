//! Loading a game behind the loading screen (`GameLoop::load`): the world, the town, its models,
//! decals and sounds, from a new map or a save.

use super::*;

impl GameplayLoop {
    /// `GameLoop::load`: build everything the game needs before it is shown.
    pub(super) fn load_world(&mut self, ctx: &mut Ctx) {
        let saved = self.saved.clone();
        let world = match &saved {
            Some(s) => World::from_save(ctx.archive, s),
            None => World::generate(ctx.archive, &self.params),
        };
        match world {
            Ok(world) => {
                let lighting = Lighting::from_month(&world.environment.months[0]);
                self.weather = world.environment.weather.clone();
                // `FUN_1400d9450` builds each slot's particles from its Weather description.
                self.precipitation = self
                    .weather
                    .iter()
                    .enumerate()
                    .map(|(i, (w, _))| {
                        let r = w.as_deref()?.replace('\\', "/");
                        let data = ctx.archive.read_ref(&r).ok()?;
                        let desc = banished_formats::weather::weather(&data).ok()?;
                        Some(crate::view::weather::Precipitation::new(
                            desc,
                            0x7ea7 + i as u32,
                        ))
                    })
                    .collect();
                self.river_paths = world.river_paths.clone();
                if saved.is_none() {
                    self.new_map = Some(new_map(&world));
                }
                self.environment_months = world.environment.months;
                self.environment_template = world.environment.template.clone();
                log::info!(
                    "{} terrain ready for {} ({}, {})",
                    world.profile.name,
                    self.params.town_name,
                    self.params.climate,
                    self.params.start_condition
                );
                // `FUN_1400a87d0`: start point, structures and natural resources, drawn from
                // the game stream before anything else consumes it.
                let grid = banished_game::terrain::HeightGrid {
                    width: world.terrain.size + 1,
                    heights: &world.terrain.heights,
                };
                let town = match &saved {
                    Some(s) => StartingTown::from_save(ctx.archive, s),
                    None => StartingTown::create(
                        ctx.archive,
                        &self.params.start_condition_ref,
                        world.terrain.size / 2,
                        &grid,
                        &mut self.rng,
                    ),
                };
                // Inferred: the camera opens over the start point (not yet traced).
                let mut camera = Camera::ground();
                if let Some(town) = &town {
                    let c = (world.terrain.size / 4) as f32 - 0.5;
                    camera.target =
                        glam::Vec2::new(town.start.0 as f32 - c, town.start.1 as f32 - c);
                }
                // A save restores its camera (`FUN_14009dd90`: target `+0x1c`, then yaw,
                // pitch and distance).
                if let Some(c) = saved.as_ref().and_then(|s| s.state.camera) {
                    camera.target = glam::Vec2::new(c.target[0], c.target[1]);
                    camera.yaw = c.floats[0];
                    camera.pitch = c.floats[1];
                    camera.distance = c.floats[2];
                }
                // The rivers and lakes with the profile's River and Lake materials.
                let w = &world.water;
                let water = crate::view::scene_assets::WaterData::from_mesh(
                    ctx.archive,
                    &world.profile.materials[1],
                    &world.profile.materials[2],
                    &w.vertices,
                    &w.indices,
                    w.rivers.clone(),
                    w.lakes.clone(),
                );
                let mut view = SceneView::new(world.terrain, SceneCamera::Map(camera), lighting);
                // `FUN_14009bfd0`: the sky (`Models/Sky/SkyMesh.rsc`); the roads' decals follow.
                view.extras = Some(crate::view::scene_assets::SceneExtras {
                    sky: crate::view::scene_assets::SkyData::load(
                        ctx.archive,
                        "Models/Sky/SkyMesh.rsc",
                    ),
                    water,
                    decals: Vec::new(),
                });
                self.road_materials = crate::gfx::road_decals::materials(ctx.archive);
                self.decal_descriptions.clear();
                if let Some(town) = &town {
                    view.models = Some(town.models(ctx.archive));
                }
                self.town = town;
                // A new game's ambience regions join the entity manager's bucket 2.
                if let (None, Some(town)) = (&saved, &mut self.town) {
                    let regions = crate::audio::ambience::region_count(
                        ctx.archive,
                        &self.params.start_condition_ref,
                        town.cells.size,
                    );
                    town.register_ambience(regions);
                }
                if let Some(town) = &mut self.town {
                    town.construction.game_flags = match &saved {
                        Some(s) => s.state.flags,
                        None if self.params.disasters => 0,
                        None => 0xf0,
                    };
                    // The trader boats' way (MapComponent `+0x1e0`'s first river).
                    town.construction.river = self.river_paths.first().cloned().unwrap_or_default();
                }
                // The ambience's sounds decode now, behind the loading screen, together with
                // the first music track; the emitters and the music still start on the first
                // gameplay frame (`update_audio`).
                let mut effects = self
                    .town
                    .as_ref()
                    .map(|town| crate::audio::ambience::preload_effects(ctx.archive, town))
                    .unwrap_or_default();
                let (month, mood) = self.music_month_mood();
                effects.extend(
                    self.music
                        .get_or_insert_with(|| crate::audio::music::MusicPlayer::load(ctx.archive))
                        .first_track(ctx.audio, month, mood),
                );
                self.view = Some(view);
                // The decals are built now too, so the renderer uploads them with the world;
                // the first frame rebuilds them only if the town changed meanwhile. The sounds
                // decode on other threads meanwhile (native).
                let archive: &banished_formats::pkg::Archive = ctx.archive;
                let effects: Vec<&str> = effects.iter().map(String::as_str).collect();
                let effects = ctx.audio.missing_effects(&effects);
                let (sounds, ()) = banished_formats::par::join(
                    || banished_audio::EffectCache::prepare(archive, &effects),
                    || self.refresh_decals(archive),
                );
                ctx.audio.add_prepared(sounds);
            }
            Err(e) => log::error!("world generation failed: {e}"),
        }
        // `FUN_14009bfd0` → `FUN_140099300`: bind the environment to the selected climate.
        let reference = self.params.climate_ref.replace('\\', "/");
        match ctx
            .archive
            .read_ref(&reference)
            .and_then(|d| climate_generator(&d))
        {
            Ok(c) => {
                let mut env = Environment::new(Climate::from(&c));
                // The weather list's descriptions (`Weather +0x5c`: fade time).
                for (slot, (weather, _)) in self.weather.iter().enumerate().take(2) {
                    env.fade_times[slot] = weather
                        .as_deref()
                        .and_then(|r| ctx.archive.read_ref(&r.replace('\\', "/")).ok())
                        .and_then(|d| banished_formats::weather::weather(&d).ok())
                        .map(|w| w.fade_time);
                }
                // Disasters off stop tornadoes (`Game +0xd0` bit 7).
                env.game_flags = if self.params.disasters { 0 } else { 0xf0 };
                if let Some(saved) = &self.saved {
                    env.game_flags = saved.state.flags;
                    saved.restore_environment(&mut env);
                }
                self.environment = Some(env);
            }
            Err(e) => log::error!("{reference}: {e}"),
        }
    }
}
