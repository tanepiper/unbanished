//! Player commands: whose command it is. A single-player game carries them out for player 0,
//! as before; in a multiplayer game each command stays with the player who gave it, which a
//! separate towns game reads to act on that player's town (docs/MULTIPLAYER-PLAN.md).

use super::generated;
use crate::gameplay::commands::PlayerCommand;

#[test]
fn a_command_is_carried_out_for_its_player() {
    let Some((mut game, mut gameplay)) = generated() else {
        return;
    };
    game.with_ctx(|_, ctx| {
        assert_eq!(gameplay.player(), 0);
        let command = PlayerCommand::SetStockLimit { slot: 2, limit: 321 };
        gameplay.execute_as(ctx.archive, 1, command);
        assert_eq!(gameplay.sim.acting_player, 1);
        assert_eq!(gameplay.sim.stock_warnings.limits[2], 321);
    });
}

/// Another player's command, carried out at its lockstep slot, is still theirs.
#[cfg(feature = "multiplayer")]
#[test]
fn a_remote_command_keeps_its_player() {
    let Some((mut game, mut gameplay)) = generated() else {
        return;
    };
    gameplay.join_lockstep(0, 2, 0);
    game.with_ctx(|_, ctx| {
        let command = PlayerCommand::SetNotify {
            category: 2,
            on: false,
        };
        gameplay.execute_from(ctx, 1, command);
        assert_eq!(gameplay.sim.acting_player, 1);
        assert!(!gameplay.sim.events.notify[2]);
    });
}
