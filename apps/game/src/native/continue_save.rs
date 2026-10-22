//! `--sim-continue SAVE ITERATIONS OUT.sav [WINDATA]`: runs a save for an exact number of
//! simulation iterations without a window, and writes the result as a save. With two saves of
//! one town made by the original game a known number of iterations apart, the continued save can
//! be compared field by field with the later one (`savegame_fields_diff`), which measures how far
//! the port's simulation is from the original's.
//!
//! Each iteration is its own frame, as at speed 1, with the game's fixed step (`FRAME_STEP`).

use unbanished_sim::simulation::Simulation;
use unbanished_sim::strings::ArchiveStrings;

use super::*;

pub fn sim_continue(args: &[String]) {
    let usage = "--sim-continue SAVE ITERATIONS OUT.sav [WINDATA]";
    let save = args.get(1).expect(usage);
    let iterations: u32 = args.get(2).and_then(|s| s.parse().ok()).expect(usage);
    let out = args.get(3).expect(usage);
    let bytes = std::fs::read(save).expect("read the save");
    let dir = data_dir(args);
    let found = crate::mods::find(&dir);
    let mut game = Game::with_mods(
        read_packages(&dir),
        found,
        Some(dir),
        crate::settings::Settings::default(),
    )
    .expect("game data");
    let saved = crate::saves::saved::SavedGame::parse(&bytes).expect("parse the save");
    let params = saved.params();
    let dt = unbanished_game::scheduler::seconds(crate::FRAME_STEP);
    let file = game.with_ctx(|_, ctx| {
        let mut sim = Simulation::from_save(ctx.archive, &saved).expect("load the save");
        sim.load_achievements(ctx.archive, Some(&saved), |_| false);
        let mut strings = ArchiveStrings::new(ctx.archive);
        for _ in 0..iterations {
            sim.frame(ctx.archive, &mut strings, &params, 1, dt, false);
        }
        let state = sim.saved_state(ctx.archive, &mut strings, &saved, &params.town_name);
        unbanished_formats::save::Save::write(&saved.metadata, &state.encode().expect("encode"))
    });
    std::fs::write(out, file).expect("write the save");
    log::info!("{iterations} iterations of {save} written to {out}");
}
