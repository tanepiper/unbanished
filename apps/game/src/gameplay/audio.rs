//! The game's music and the scene's sound each frame.

use super::*;

impl GameplayLoop {
    /// The music player's month and town mood (`FUN_1400a95b0`).
    pub(super) fn music_month_mood(&self) -> (i32, f32) {
        let v = self.status_values();
        let people = v.adults + v.students + v.children;
        let mood =
            crate::audio::music::mood(v.health, v.health_max, v.happiness, v.happiness_max, people);
        (self.month() as i32, mood)
    }

    /// Whether the music player and `Scene::Update` draw from the game's Random, as in the
    /// original (`FUN_14009b850` passes `Game +0xd8`): each sound that starts draws twice
    /// (`Audio::Scene::Update`, Runtime `0x1800e1f10`), so UI clicks, placement sounds and the
    /// ambience move the town's Random, at times that depend on the frame rate.
    // Deviation: a multiplayer game, a recording, a replay and `--sim-digest` draw them from
    // the audio's own Random, because clicks and frame times differ between computers and
    // between runs, and the town must stay the same on each.
    pub(super) fn audio_draws_from_game(&self) -> bool {
        !(self.audio_own_random || self.online() || self.recording() || self.replaying())
    }

    /// `FUN_14009b850`'s audio: the music player (`FUN_1400a95b0`), then
    /// `Scene::Update(dt, dt × speed × 6, random, listener, camera distance)`, the listener being
    /// the camera pivot at the eye's height.
    pub(super) fn update_audio(&mut self, ctx: &mut Ctx, dt: f32) {
        let (month, mood) = self.music_month_mood();
        let shared = self.audio_draws_from_game();
        let music = self
            .music
            .get_or_insert_with(|| crate::audio::music::MusicPlayer::load(ctx.archive));
        let random = shared.then_some(&mut self.sim.rng);
        music.update(ctx.audio, ctx.archive, month, mood, random);
        let temperature = self
            .sim
            .environment
            .as_ref()
            .map_or(50.0, |e| e.temperature);
        if let Some(town) = &self.sim.town {
            match &mut self.ambience {
                None => {
                    self.ambience = Some(crate::audio::ambience::Ambience::create(
                        ctx.audio,
                        ctx.archive,
                        &self.params.start_condition_ref,
                        town,
                        &self.river_paths,
                        temperature,
                    ));
                }
                Some(a) => {
                    if std::mem::take(&mut self.ambience_stale) {
                        a.reweigh(town);
                        a.sync_entities(ctx.audio, ctx.archive, town);
                    }
                    let particles = &self.particles;
                    a.follow_tornadoes(ctx.audio, town, |e| particles.alpha((e, 0)));
                    a.drive_entities(ctx.audio, town);
                    a.fade(ctx.audio, temperature);
                }
            }
        }
        // The precipitation emitters follow the environment's effects (`FUN_1400999b0`); with
        // the game option ShowWeather (`+0x16`) off the effects and their emitters are removed.
        // ShowWeather also gates the weather lighting blend (`FUN_140098f90`, in `update`).
        let show_weather = ctx.settings.game.show_weather;
        if let (Some(a), Some(env)) = (&mut self.ambience, &self.sim.environment) {
            for (slot, (_, ambient)) in self.weather.iter().enumerate().take(2) {
                let intensity = env.effects[slot]
                    .filter(|_| show_weather)
                    .map(|e| e.intensity);
                a.weather(ctx.audio, ctx.archive, slot, ambient.as_deref(), intensity);
            }
        }
        for (at, effect) in std::mem::take(&mut self.pending_sounds) {
            ctx.audio.play_3d(
                ctx.archive,
                &effect.replace('\\', "/"),
                crate::audio::sound::sound_type::EFFECT,
                at,
            );
        }
        let Some(SceneView {
            camera: SceneCamera::Map(camera),
            terrain,
            ..
        }) = &self.view
        else {
            return;
        };
        // The listener is the pivot's x and y at the eye's height: `FUN_14009b850` copies the
        // camera's Transform3 (`+0x98`) and takes z from its offset (`+0x2c`, the eye at
        // `+0xc4`, as `FUN_140107c90` reads it). High eyes hear less of the far town.
        let listener = camera.target.extend(camera.eye(terrain).z);
        // The step reads the iteration count (`+0x34`) whether or not the game is paused.
        let speed = SPEED_LEVELS[self.speed as usize] as f32;
        let random = shared.then_some(&mut self.sim.rng);
        ctx.audio
            .update(dt, dt * speed * 6.0, listener, camera.distance, random);
    }
}
