use super::*;

mod audio;
mod drawing;
mod tools;
mod tutorial;
mod windows;

fn generated() -> Option<(crate::Game, GameplayLoop)> {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../game-data/original/WinData");
    if !dir.join("data0.pkg").exists() {
        eprintln!("game data missing; skipping gameplay growth integration");
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
    let mut game =
        crate::Game::with_settings(packages, crate::settings::Settings::default()).unwrap();
    let mut gameplay = GameplayLoop::new(WorldParams {
        start_condition_ref: r"Game\StartCondition\Medium.rsc:resource".into(),
        ..WorldParams::default()
    });
    game.with_ctx(|_, ctx| {
        gameplay.load(ctx);
        gameplay.startup(ctx);
    });
    Some((game, gameplay))
}
