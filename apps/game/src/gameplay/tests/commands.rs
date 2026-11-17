//! Player commands: whose command it is. A single-player game carries them out for player 0,
//! as before; in a multiplayer game each command stays with the player who gave it, which a
//! separate towns game reads to act on that player's town (docs/MULTIPLAYER-PLAN.md).

use super::generated;
use crate::gameplay::commands::PlayerCommand;
use crate::loops::GameLoop;

#[test]
fn a_command_is_carried_out_for_its_player() {
    let Some((mut game, mut gameplay)) = generated() else {
        return;
    };
    game.with_ctx(|_, ctx| {
        assert_eq!(gameplay.player(), 0);
        let command = PlayerCommand::SetStockLimit {
            slot: 2,
            limit: 321,
        };
        gameplay.execute_as(ctx.archive, 1, command);
        assert_eq!(gameplay.sim.acting_player, 1);
        assert_eq!(gameplay.sim.stock_warnings[0].limits[2], 321);
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

/// What a separate towns player's command places belongs to that player; the world's own and a
/// shared town's belong to player 0, as before.
#[test]
fn a_placed_building_keeps_its_players_owner() {
    let Some((mut game, mut gameplay)) = generated() else {
        return;
    };
    game.with_ctx(|_, ctx| {
        assert!(gameplay.script(ctx, "tool Template/ForesterLodge.rsc"));
        let start = gameplay.sim.town.as_ref().unwrap().start;
        let view = gameplay.view.as_ref().unwrap();
        let grid = unbanished_game::terrain::HeightGrid {
            width: view.terrain.size + 1,
            heights: &view.terrain.heights,
        };
        let tool = gameplay.placement.as_mut().unwrap();
        tool.hover(gameplay.sim.town.as_ref().unwrap(), &grid, Some(start), 0);
        let placement = tool.placement(0).expect("a valid placement");
        // Two starts: a separate towns game, whose commands make their player's buildings.
        gameplay.sim.town.as_mut().unwrap().starts = vec![start, (start.0 + 40, start.1)];
        gameplay.execute_as(
            ctx.archive,
            1,
            PlayerCommand::SetStockLimit { slot: 1, limit: 1 },
        );
        let town = gameplay.sim.town.as_mut().unwrap();
        assert_eq!(town.owner_of_command, 1);
        let id = unbanished_sim::tools::placement_tool::place(ctx.archive, town, &placement);
        assert_eq!(town.placed[id].owner, 1);
        // Nothing is being carried out now: the next one is the world's own, owner 0.
        town.owner_of_command = 0;
        let other = unbanished_sim::tools::placement_tool::place(ctx.archive, town, &placement);
        assert_eq!(town.placed[other].owner, 0);
    });
}

/// A command carries its player into the town only in a separate towns game: a shared town's
/// one town makes every player's buildings its own.
#[test]
fn only_a_separate_towns_game_keeps_a_players_commands() {
    let Some((mut game, mut gameplay)) = generated() else {
        return;
    };
    game.with_ctx(|_, ctx| {
        let command = || PlayerCommand::SetStockLimit { slot: 1, limit: 7 };
        gameplay.execute_as(ctx.archive, 3, command());
        let town = gameplay.sim.town.as_ref().unwrap();
        assert_eq!(town.owner_of_command, 0);
        let start = town.start;
        gameplay.sim.town.as_mut().unwrap().starts = vec![start, (start.0 + 40, start.1)];
        gameplay.execute_as(ctx.archive, 3, command());
        let town = gameplay.sim.town.as_ref().unwrap();
        assert_eq!(town.owner_of_command, 3);
    });
}

/// Each player's stock is what their own storages hold.
#[test]
fn stock_belongs_to_the_players_who_own_the_storages() {
    let Some((mut game, mut gameplay)) = generated() else {
        return;
    };
    game.with_ctx(|_, ctx| {
        let sites: Vec<(usize, usize)> = gameplay
            .sim
            .town
            .as_ref()
            .unwrap()
            .storage_sites
            .iter()
            .map(|s| (s.entity, s.storage))
            .collect();
        let count = |items: &[unbanished_game::work::stock::Item]| -> i32 {
            items.iter().map(|i| i.count).sum()
        };
        let total: i32 = sites
            .iter()
            .map(|&(_, storage)| {
                count(&gameplay.sim.town.as_ref().unwrap().storages[storage].items)
            })
            .sum();
        assert!(total > 0, "the start's storages hold the starting goods");
        for &(entity, _) in &sites {
            gameplay.sim.town.as_mut().unwrap().placed[entity].owner = 1;
        }
        // The town is a separate towns one now, so each player has a stock table.
        gameplay.sim.set_players(2);
        gameplay.update(ctx, 1.0 / 30.0);
        let w = &gameplay.sim.stock_warnings;
        assert_eq!(w[0].count.iter().sum::<i32>(), 0);
        assert!(w[1].count.iter().sum::<i32>() > 0);
    });
}
