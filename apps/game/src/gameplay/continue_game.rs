//! A save continued through the whole game loop without a window (`--sim-continue --game`).
//!
//! `--sim-continue` alone runs the town's simulation. The original also draws from the town's
//! Random outside it, even with no sound device: the music player picks its next song and the
//! audio scene picks each sound's variant and pitch (*measured* in CrossOver with XAudio2 off,
//! docs/ITERATION-ORDER.md). This runs the game loop, which makes those draws too, so its draws
//! can be compared one by one with the original's.
//!
//! Deviation: not in the original; a check run from the command line.

use crate::loops::GameLoop;

/// Loads `save` into `game`'s context and runs `iterations` iterations at the speed level
/// `speed` (1, 2, 5 or 10 iterations a frame), with no autosave and the sounds drawn from the
/// town's Random. `clock` seeds the temperature noise as the original's load does
/// (`Environment::seed_noise_from_clock`). Returns the save file.
pub fn run(
    game: &mut crate::Game,
    save: &[u8],
    iterations: u32,
    speed: u32,
    clock: Option<u32>,
    draws: Option<&std::path::Path>,
) -> Result<Vec<u8>, String> {
    let level = super::SPEED_LEVELS
        .iter()
        .position(|&l| l == speed)
        .ok_or("--speed must be 1, 2, 5 or 10 with --game")?;
    let saved = crate::saves::saved::SavedGame::parse(save).map_err(|e| e.to_string())?;
    let metadata = saved.metadata.clone();
    let mut gameplay = super::GameplayLoop::from_save(saved);
    let step = unbanished_game::scheduler::seconds(crate::FRAME_STEP);
    game.with_ctx(|_, ctx| {
        gameplay.load(ctx);
        gameplay.startup(ctx);
        if let (Some(clock), Some(env)) = (clock, gameplay.sim.environment.as_mut()) {
            env.seed_noise_from_clock(clock);
        }
        gameplay.speed = level as u8;
        gameplay.paused = false;
        // No autosave: it would wait for a drawn preview and write the player's saves.
        gameplay.autosave = None;
        gameplay.audio_own_random = false;
        if draws.is_some() {
            unbanished_game::random::trace::start();
        }
        for _ in 0..iterations / speed {
            gameplay.update(ctx, step);
        }
        if let Some(path) = draws {
            let d = unbanished_game::random::trace::take();
            unbanished_game::random::trace::write(path, &d).map_err(|e| e.to_string())?;
        }
        let state = gameplay
            .save_state(ctx.archive, ctx.ui)
            .ok_or("no game state")?;
        let state = state.encode().map_err(|e| e.to_string())?;
        Ok(unbanished_formats::save::Save::write(&metadata, &state))
    })
}
