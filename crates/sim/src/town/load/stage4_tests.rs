use super::tests_support::*;
use unbanished_formats::savegame::Value;

#[test]
fn vanilla_saves_restore_environment_and_logs() {
    let Some(archive) = archive() else {
        eprintln!("game data missing; skipping");
        return;
    };
    for name in [
        "Save2.sav",
        "Save3.sav",
        "Save4.sav",
        "Save5.sav",
        "Save83.sav",
    ] {
        let Ok(bytes) = std::fs::read(root().join("save-game").join(name)) else {
            continue;
        };
        let saved = crate::saved::SavedGame::parse(&bytes).unwrap();
        let climate = saved
            .reference(saved.state.climate)
            .unwrap()
            .replace('\\', "/");
        let generator =
            unbanished_formats::game::climate_generator(&archive.read_ref(&climate).unwrap())
                .unwrap();
        let mut env = unbanished_game::nature::environment::Environment::new(
            unbanished_game::nature::environment::Climate::from(&generator),
        );
        saved.restore_environment(&mut env);
        let m = &saved.state.managers;
        assert_eq!(
            Some(env.temperature),
            m.f32("environment.temperature"),
            "{name}"
        );
        assert_eq!(
            env.season.snow,
            (32.0 - env.temperature).clamp(0.0, 1.0),
            "{name}: snow"
        );
        let log = saved.event_log();
        let entries = match m.at("event_log.entries") {
            Some(Value::List(v)) => v.len(),
            _ => 0,
        };
        assert_eq!(log.entries.len(), entries, "{name}: events");
        assert!(
            log.entries.iter().all(|e| e.text.is_some()),
            "{name}: event texts"
        );
        let stats = saved.statistics();
        let history = match saved.state.town.at("town.history") {
            Some(Value::List(v)) => v.len(),
            _ => 0,
        };
        assert!(history <= stats.series.len(), "{name}: {history} series");
        eprintln!(
            "{name}: {:.1}°F state {} level {}, {} events, {} samples in the first series",
            env.temperature,
            env.state,
            env.level,
            log.entries.len(),
            stats.series.first().map_or(0, |s| s.len())
        );
    }
}
