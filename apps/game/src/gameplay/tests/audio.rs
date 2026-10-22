//! Music, ambience, the pause menu's sounds, rain and building emitters.

use super::*;

#[test]
fn audio_plays_music_ambience_and_keeps_the_simulation_stream() {
    let Some((mut game, mut gameplay)) = generated() else {
        return;
    };
    game.with_ctx(|_, ctx| {
        ctx.audio.set_master_volume(0.0);
        for _ in 0..3 {
            gameplay.update(ctx, 1.0 / 30.0);
        }
        let (effects, effects3d) = ctx.audio.scene.active();
        eprintln!("2D effects {effects}, 3D effects {effects3d}");
        // Environment classes (warm and cold) and the wind share 3D effects.
        assert!(effects3d >= 2, "{effects3d}");
        // The music track and the audible ambience.
        assert!(effects >= 1, "{effects}");
        let ambience = gameplay.ambience.as_ref().unwrap();
        assert!(ambience.region_count() > 0);
        // Every generated map has a river (`FUN_1400d2560`): emitters every 8 units.
        let length: f32 = gameplay
            .river_paths
            .iter()
            .flat_map(|p| p.windows(2))
            .map(|w| (glam::Vec2::from(w[1]) - glam::Vec2::from(w[0])).length())
            .sum();
        eprintln!("river length {length}, emitters {}", ambience.river_count());
        assert!(ambience.river_count() > 0);
        assert!(ambience.river_count() as f32 <= length / 8.0);
    });
}

#[test]
fn pause_menu_pauses_sounds_and_the_simulation() {
    use crate::audio::sound::sound_type;
    let Some((mut game, mut gameplay)) = generated() else {
        return;
    };
    game.with_ctx(|_, ctx| {
        ctx.audio.set_master_volume(0.0);
        gameplay.update(ctx, 1.0 / 30.0);
        gameplay.raw(ctx, RawInput::Escape);
        assert!(gameplay.pause_menu.is_some());
        for kind in [sound_type::MUSIC, sound_type::AMBIENT, sound_type::EFFECT] {
            assert!(ctx.audio.scene.is_paused(kind));
        }
        assert!(!ctx.audio.scene.is_paused(sound_type::UI));
        let (rng, seconds) = (gameplay.sim.rng.clone(), gameplay.sim.calendar.seconds());
        for _ in 0..3 {
            gameplay.update(ctx, 1.0 / 30.0);
        }
        assert_eq!(gameplay.sim.calendar.seconds(), seconds);
        assert_eq!(format!("{:?}", gameplay.sim.rng), format!("{rng:?}"));
        // Escape is buttonResume's action.
        gameplay.raw(ctx, RawInput::Escape);
        assert!(gameplay.pause_menu.is_none());
        assert!(!ctx.audio.scene.is_paused(sound_type::MUSIC));
        gameplay.update(ctx, 1.0 / 30.0);
        assert!(gameplay.sim.calendar.seconds() > seconds);
    });
}

#[test]
fn rain_effect_drives_the_rain_emitter() {
    let Some((mut game, mut gameplay)) = generated() else {
        return;
    };
    assert_eq!(
        gameplay.sim.environment.as_ref().unwrap().fade_times[0],
        Some(20.0)
    );
    game.with_ctx(|_, ctx| {
        ctx.audio.set_master_volume(0.0);
        gameplay.update(ctx, 1.0 / 30.0);
        let (_, before) = ctx.audio.scene.active();
        gameplay.sim.environment.as_mut().unwrap().effects[0] =
            Some(unbanished_game::nature::environment::WeatherEffect {
                intensity: 0.5,
                direction: 1.0,
            });
        gameplay.update(ctx, 1.0 / 30.0);
        let ambience = gameplay.ambience.as_ref().unwrap();
        assert!(ambience.weather_active(0));
        // The rain emitter's effect plays (other 3D one-shots come and go with the game
        // stream, so the scene's total is not compared).
        let handles = ambience.weather_handles(0);
        assert!(handles
            .iter()
            .any(|&h| ctx.audio.scene.emitter_count(h) > 0));
        let _ = before;
        // A save made now has the rain emitter's entity, its first slot at the intensity.
        let w = ambience.saved().weather;
        assert_eq!(w.len(), 1);
        assert!(w[0].0.contains("RainEffect"), "{}", w[0].0);
        assert!((w[0].2[0] - 0.5).abs() < 0.01, "{:?}", w[0].2);
        let g = gameplay.save_state(ctx.archive, ctx.ui).unwrap();
        let rain = g.entities.iter().flatten().filter(|&&e| {
            let o = &g.objects[&e];
            matches!(o.fields.at("entity.template"),
                Some(unbanished_formats::savegame::Value::Ext(r))
                    if g.references[r].contains("RainEffect"))
        });
        assert_eq!(rain.count(), 1);
        // Rain stops and cannot start again in this step (whatever the stream draws).
        let env = gameplay.sim.environment.as_mut().unwrap();
        env.effects[0] = None;
        env.fade_times[0] = None;
        gameplay.update(ctx, 1.0 / 30.0);
        assert!(!gameplay.ambience.as_ref().unwrap().weather_active(0));
        assert!(handles
            .iter()
            .all(|&h| ctx.audio.scene.emitter_count(h) == 0));
    });
}

#[test]
fn building_emitters_register_without_playing() {
    let Some((mut game, mut gameplay)) = generated() else {
        return;
    };
    game.with_ctx(|_, ctx| {
        ctx.audio.set_master_volume(0.0);
        gameplay.update(ctx, 1.0 / 30.0);
        let town = gameplay.sim.town.as_ref().unwrap();
        let building = town
            .placed
            .iter()
            .position(|p| {
                p.alive
                    && ctx
                        .archive
                        .read_ref(&format!(
                            "{}:ambientemitter",
                            town.templates[p.template]
                                .reference
                                .replace('\\', "/")
                                .split(':')
                                .next()
                                .unwrap()
                        ))
                        .is_ok()
            })
            .expect("a building with an ambient emitter");
        let at = town.placed[building].position;
        let ambience = gameplay.ambience.as_mut().unwrap();
        assert!(ambience.entity_emitters() > 0);
        // Autostart is off: nothing plays until building state starts a slot (fire).
        let (_, before) = ctx.audio.scene.active();
        ambience.start_entity_slot(ctx.audio, building, 0, glam::Vec3::from(at), None);
        assert_eq!(ctx.audio.scene.active().1, before + 1);
        ambience.stop_entity_slot(ctx.audio, building, 0);
        assert_eq!(ctx.audio.scene.active().1, before);
    });
}
