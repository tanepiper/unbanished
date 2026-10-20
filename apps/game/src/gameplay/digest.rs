//! Digests of the simulation, to compare the same game on different platforms (native and the
//! browser). A save is loaded without a window and run at the highest game speed, and every few
//! updates the whole game state it would save is encoded and hashed. Two platforms that agree
//! write the same digests; the first that differs gives the update to look at. See
//! docs/CROSS-PLATFORM.md.
//!
//! Deviation: not in the original; a check run from the command line (`--sim-digest`) or the
//! browser build (`?digest=`).

use crate::loops::GameLoop;

/// One digest: after `update` updates (each one 1/60 s at speed 10, so ten simulation
/// iterations), the year, the time in the year in ticks, the living citizens, and the CRC-32 of
/// the encoded game state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Digest {
    pub update: u32,
    pub year: u32,
    pub elapsed: i64,
    pub citizens: usize,
    pub crc: u32,
}

impl std::fmt::Display for Digest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "update {:>6}  year {} tick {:>10}  citizens {:>4}  state {:08x}",
            self.update, self.year, self.elapsed, self.citizens, self.crc
        )
    }
}

/// Loads `save` into `game`'s context and runs it for `updates` updates at speed 10, with a
/// digest before the first update and after every `every` updates.
pub fn run(
    game: &mut crate::Game,
    save: &[u8],
    updates: u32,
    every: u32,
) -> Result<Vec<Digest>, String> {
    let saved = crate::saves::saved::SavedGame::parse(save).map_err(|e| e.to_string())?;
    let mut gameplay = super::GameplayLoop::from_save(saved);
    let step = unbanished_game::scheduler::seconds(crate::FRAME_STEP);
    game.with_ctx(|_, ctx| {
        gameplay.load(ctx);
        gameplay.startup(ctx);
        gameplay.speed = 3;
        gameplay.paused = false;
        // No autosave: it would wait for a drawn preview and write the player's saves.
        gameplay.autosave = None;
        let mut out = vec![gameplay.digest(ctx, 0)?];
        for update in 1..=updates {
            gameplay.update(ctx, step);
            if update % every.max(1) == 0 {
                out.push(gameplay.digest(ctx, update)?);
            }
        }
        Ok(out)
    })
}

impl super::GameplayLoop {
    pub(super) fn digest(
        &self,
        ctx: &mut crate::loops::Ctx,
        update: u32,
    ) -> Result<Digest, String> {
        let mut state = self
            .save_state(ctx.archive, ctx.ui)
            .ok_or("no game state")?;
        // A save also holds each player's own view, which is not the game: the camera, and the
        // settings each building's window remembers (UIComponent). The digest leaves them out.
        state.camera = self.saved.as_ref().and_then(|s| s.state.camera.clone());
        for object in state.objects.values_mut() {
            if object.type_name == "UIComponent" {
                object.fields.set(
                    "ui.pages",
                    unbanished_formats::savegame::Value::List(Vec::new()),
                );
            }
        }
        let state = state.encode().map_err(|e| e.to_string())?;
        let town = self.sim.town.as_ref().ok_or("no town")?;
        Ok(Digest {
            update,
            year: self.sim.calendar.year,
            elapsed: self.sim.calendar.elapsed,
            citizens: town.citizens.iter().filter(|c| c.alive).count(),
            crc: unbanished_formats::crc::crc32(state),
        })
    }
}
