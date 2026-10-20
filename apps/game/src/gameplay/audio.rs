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

    /// `FUN_14009b850`'s audio: the music player (`FUN_1400a95b0`), then
    /// `Scene::Update(dt, dt × speed × 6, random, camera pivot, camera distance)`.
    pub(super) fn update_audio(&mut self, ctx: &mut Ctx, dt: f32) {
        let (month, mood) = self.music_month_mood();
        let music = self
            .music
            .get_or_insert_with(|| crate::audio::music::MusicPlayer::load(ctx.archive));
        music.update(ctx.audio, ctx.archive, month, mood);
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
            ..
        }) = &self.view
        else {
            return;
        };
        let listener = camera.target.extend(0.0);
        // The step reads the iteration count (`+0x34`) whether or not the game is paused.
        let speed = SPEED_LEVELS[self.speed as usize] as f32;
        ctx.audio
            .update(dt, dt * speed * 6.0, listener, camera.distance);
    }
}
