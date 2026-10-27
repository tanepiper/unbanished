//! `--sim-continue SAVE ITERATIONS OUT.sav [WINDATA] [--speed N] [--clock SECONDS] [--draws FILE]
//! [--game]`: runs a save for an exact number of simulation iterations without a window, and
//! writes the result as a save. With two saves of one town made by the original game a known
//! number of iterations apart, the continued save can be compared field by field with the later
//! one (`savegame_fields_diff`), which measures how far the port's simulation is from the
//! original's.
//!
//! Each frame runs `--speed` iterations (1 by default, as at speed 1), with the game's fixed step
//! (`FRAME_STEP`). The speed matters: the steps after a frame's iterations draw from the game's
//! Random once a frame (the seeding's `smooth`), so it must be the speed the original ran at.
//! `ITERATIONS` must be a multiple of it.
//!
//! `--clock` is the wall clock (seconds since 1970) when the original loaded the save. The
//! original seeds a loaded game's temperature noise from it (`seed_noise_from_clock`), and the
//! weather follows that noise until the next reseed. Without it the port keeps its fixed seed.
//!
//! `--draws` writes every draw from the game's Random during the iterations, one per line
//! (`unbanished_game::random::trace`), for `tools/triage/rngdiff.py` to compare with the
//! original's.
//!
//! `--replay-draws FILE` (with `--draws`) makes plain draws before each recorded draw, one number per line
//! (`tools/triage/rng_split.py`): the original's sound draws, so the simulation gets the same
//! numbers as the original's without running its sounds.
//!
//! `--loiter-picks FILE` gives the map rectangle (`x y w h` per line) of each object the
//! original's Loiter picked, in order (`tools/triage/loiter_picks.py`), so the comparison goes past
//! the picks whose set order differs (`unbanished_game::behaviour::loiter_order`).
//!
//! `--game` runs the whole game loop instead of the simulation alone
//! (`gameplay::continue_game`), so the music and the sounds draw from the town's Random as the
//! original's do. Its speed must be one of the game's levels.

use unbanished_sim::simulation::Simulation;
use unbanished_sim::strings::ArchiveStrings;

use super::*;

pub fn sim_continue(args: &[String]) {
    let usage = "--sim-continue SAVE ITERATIONS OUT.sav [WINDATA] [--speed N] [--clock SECONDS] [--draws FILE] [--replay-draws FILE] [--loiter-picks FILE] [--game]";
    let save = args.get(1).expect(usage);
    let iterations: u32 = args.get(2).and_then(|s| s.parse().ok()).expect(usage);
    let out = args.get(3).expect(usage);
    let speed: u32 = match args.iter().position(|a| a == "--speed") {
        Some(i) => args.get(i + 1).and_then(|s| s.parse().ok()).expect(usage),
        None => 1,
    };
    let clock: Option<u32> = args
        .iter()
        .position(|a| a == "--clock")
        .map(|i| args.get(i + 1).and_then(|s| s.parse().ok()).expect(usage));
    let draws = args
        .iter()
        .position(|a| a == "--draws")
        .map(|i| args.get(i + 1).expect(usage));
    assert!(
        speed > 0 && iterations.is_multiple_of(speed),
        "ITERATIONS must be a multiple of --speed"
    );
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
    if args.iter().any(|a| a == "--game") {
        game.audio.set_master_volume(0.0);
        let draws = draws.map(std::path::Path::new);
        match crate::gameplay::continue_game::run(
            &mut game, &bytes, iterations, speed, clock, draws,
        ) {
            Ok(file) => std::fs::write(out, file).expect("write the save"),
            Err(e) => {
                eprintln!("--sim-continue --game: {e}");
                std::process::exit(1);
            }
        }
        log::info!("{iterations} iterations of {save} through the game loop written to {out}");
        return;
    }
    let saved = crate::saves::saved::SavedGame::parse(&bytes).expect("parse the save");
    let params = saved.params();
    let dt = unbanished_game::scheduler::seconds(crate::FRAME_STEP);
    let file = game.with_ctx(|_, ctx| {
        let mut sim = Simulation::from_save(ctx.archive, &saved).expect("load the save");
        sim.load_achievements(ctx.archive, Some(&saved), |_| false);
        if let (Some(clock), Some(env)) = (clock, sim.environment.as_mut()) {
            env.seed_noise_from_clock(clock);
        }
        let mut strings = ArchiveStrings::new(ctx.archive);
        if draws.is_some() {
            unbanished_game::random::trace::start();
        }
        if let Some(i) = args.iter().position(|a| a == "--replay-draws") {
            let text = std::fs::read_to_string(args.get(i + 1).expect(usage)).expect("read");
            let skips = text
                .lines()
                .map(|l| l.trim().parse::<u32>().expect("a number"));
            unbanished_game::random::trace::replay(skips);
        }
        if let Some(i) = args.iter().position(|a| a == "--loiter-picks") {
            let text = std::fs::read_to_string(args.get(i + 1).expect(usage)).expect("read");
            let picks = text.lines().map(|l| {
                let v: Vec<i32> = l
                    .split_whitespace()
                    .map(|n| n.parse().expect("a number"))
                    .collect();
                (v[0], v[1], v[2], v[3])
            });
            unbanished_game::behaviour::loiter_order::set(picks);
        }
        for _ in 0..iterations / speed {
            sim.frame(ctx.archive, &mut strings, &params, speed, dt, false);
        }
        if let Some(path) = draws {
            let d = unbanished_game::random::trace::take();
            unbanished_game::random::trace::write(std::path::Path::new(path), &d)
                .expect("write the draws");
        }
        let state = sim.saved_state(ctx.archive, &mut strings, &saved, &params.town_name);
        unbanished_formats::save::Save::write(&saved.metadata, &state.encode().expect("encode"))
    });
    std::fs::write(out, file).expect("write the save");
    log::info!("{iterations} iterations of {save} at speed {speed} written to {out}");
}
