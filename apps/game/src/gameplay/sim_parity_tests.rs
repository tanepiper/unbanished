//! The simulation alone plays a save as the game does: `Simulation::from_save` and
//! `Simulation::frame`, with no window, view or interface, give the digests of
//! `--sim-digest` (`digest::run`, the whole game loop).

use unbanished_sim::simulation::Simulation;

use super::digest;

fn game() -> Option<crate::Game> {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../game-data/original/WinData");
    if !dir.join("data0.pkg").exists() {
        eprintln!("game data missing; skipping");
        return None;
    }
    let packages = ["data1.pkg", "patch1.pkg", "patch0.pkg", "data0.pkg"]
        .into_iter()
        .filter_map(|name| {
            std::fs::read(dir.join(name))
                .ok()
                .map(|data| (name.to_owned(), data))
        })
        .collect();
    crate::Game::with_settings(packages, crate::settings::Settings::default()).ok()
}

/// The CRC-32 of the sim's state before the first update and after every `every` updates, at
/// speed 10 as `digest::run` plays.
fn sim_digests(game: &mut crate::Game, save: &[u8], updates: u32, every: u32) -> Vec<u32> {
    let saved = crate::saves::saved::SavedGame::parse(save).unwrap();
    let params = saved.params();
    let dt = unbanished_game::scheduler::seconds(crate::FRAME_STEP);
    game.with_ctx(|_, ctx| {
        let mut sim = Simulation::from_save(ctx.archive, &saved).unwrap();
        // The test game's settings store has nothing awarded.
        sim.load_achievements(ctx.archive, Some(&saved), |_| false);
        let auto_slow = ctx.settings.game.auto_slow;
        let mut speed = 3;
        let mut strings = crate::ui::strings::UiStrings(&mut ctx.ui.res);
        let digest = |sim: &Simulation, strings: &mut crate::ui::strings::UiStrings| {
            let mut state = sim.saved_state(ctx.archive, strings, &saved, &params.town_name);
            for object in state.objects.values_mut() {
                if object.type_name == "UIComponent" {
                    object.fields.set(
                        "ui.pages",
                        unbanished_formats::savegame::Value::List(Vec::new()),
                    );
                }
            }
            unbanished_formats::crc::crc32(state.encode().unwrap())
        };
        let mut out = vec![digest(&sim, &mut strings)];
        for update in 1..=updates {
            let iterations = super::SPEED_LEVELS[speed];
            let frame = sim.frame(
                ctx.archive,
                &mut strings,
                &params,
                iterations,
                dt,
                auto_slow,
            );
            if frame.slow {
                speed = 0;
            }
            if update % every == 0 {
                out.push(digest(&sim, &mut strings));
            }
        }
        out
    })
}

#[test]
fn the_simulation_alone_plays_a_save_as_the_game_does() {
    let Some(mut game) = game() else { return };
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../save-game");
    for name in ["Save83.sav", "Goldfoothill.sav"] {
        let Ok(bytes) = std::fs::read(root.join(name)) else {
            eprintln!("{name} missing; skipping it");
            continue;
        };
        let app: Vec<u32> = digest::run(&mut game, &bytes, 300, 60)
            .unwrap()
            .iter()
            .map(|d| d.crc)
            .collect();
        let sim = sim_digests(&mut game, &bytes, 300, 60);
        assert_eq!(
            sim, app,
            "{name}: the simulation alone differs from the game"
        );
    }
}
