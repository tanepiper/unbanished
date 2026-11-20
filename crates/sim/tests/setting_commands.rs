//! A resource limit and an event-log notification are player commands, so every computer of a
//! multiplayer game, and a replay, changes them at the same update. Before they were commands,
//! the window wrote them straight into the simulation, on the player's own computer only.

use unbanished_formats::pkg::Archive;
use unbanished_sim::command::PlayerCommand;
use unbanished_sim::simulation::Simulation;

fn commands() -> Vec<PlayerCommand> {
    vec![
        PlayerCommand::SetStockLimit { slot: 3, limit: 42 },
        PlayerCommand::SetNotify {
            category: 0,
            on: true,
        },
        PlayerCommand::SetNotify {
            category: 2,
            on: false,
        },
    ]
}

#[test]
fn setting_commands_change_the_simulation() {
    let archive = Archive::default();
    let mut sim = Simulation::new(1);
    for command in commands() {
        sim.execute(&archive, 0, command, None);
    }
    assert_eq!(sim.stock_warnings[0].limits[3], 42);
    assert!(sim.local_notify()[0]);
    assert!(!sim.local_notify()[2]);
}

#[test]
fn two_games_given_the_same_commands_agree() {
    let archive = Archive::default();
    let mut games = [Simulation::new(1), Simulation::new(1)];
    for sim in &mut games {
        for command in commands() {
            sim.execute(&archive, 0, command, None);
        }
    }
    assert_eq!(
        games[0].stock_warnings[0].limits,
        games[1].stock_warnings[0].limits
    );
    assert_eq!(games[0].notify, games[1].notify);
}

#[test]
fn a_slot_or_category_out_of_range_does_nothing() {
    let archive = Archive::default();
    let mut sim = Simulation::new(1);
    let (limits, notify) = (sim.stock_warnings[0].limits, sim.notify[0]);
    sim.execute(
        &archive,
        0,
        PlayerCommand::SetStockLimit {
            slot: limits.len(),
            limit: 1,
        },
        None,
    );
    sim.execute(
        &archive,
        0,
        PlayerCommand::SetNotify {
            category: notify.len(),
            on: !notify[0],
        },
        None,
    );
    assert_eq!(sim.stock_warnings[0].limits, limits);
    assert_eq!(sim.notify[0], notify);
}

#[test]
fn a_command_carries_its_giver() {
    let archive = Archive::default();
    let mut sim = Simulation::new(1);
    sim.execute(
        &archive,
        2,
        PlayerCommand::SetStockLimit { slot: 0, limit: 7 },
        None,
    );
    assert_eq!(sim.acting_player, 2);
    assert_eq!(sim.stock_warnings[0].limits[0], 7);
}

#[test]
fn a_shared_town_shares_its_stock_limits() {
    // A shared town game has one town: every player's limit is that town's.
    let archive = Archive::default();
    let mut sim = Simulation::new(1);
    sim.set_players(2);
    assert_eq!(sim.stock_warnings.len(), 1);
    sim.execute(
        &archive,
        1,
        PlayerCommand::SetStockLimit { slot: 3, limit: 9 },
        None,
    );
    assert_eq!(sim.stock_warnings[0].limits[3], 9);
}

/// A separate towns game keeps one set of notify flags per player: one player's checkbox does
/// not silence another's, and the window shows the local player's.
#[test]
fn each_towns_player_keeps_its_own_notify_flags() {
    let archive = Archive::default();
    let mut sim = Simulation::new(1);
    // The town (which decides the mode) is not loaded; a separate towns game's resize gives
    // each player a set, which two sets stand for here.
    sim.notify.push(unbanished_game::events::NOTIFY_DEFAULT);
    sim.execute(
        &archive,
        1,
        PlayerCommand::SetNotify {
            category: 2,
            on: false,
        },
        None,
    );
    assert!(!sim.notify[1][2]);
    assert!(sim.notify[0][2], "the first player's flags are untouched");
    sim.local_player = 1;
    assert!(!sim.local_notify()[2]);
    sim.local_player = 0;
    assert!(sim.local_notify()[2]);
}

/// A shared town, like a single-player game, has one set: any player's command changes it.
#[test]
fn a_shared_towns_notify_flags_are_one_set() {
    let archive = Archive::default();
    let mut sim = Simulation::new(1);
    sim.execute(
        &archive,
        3,
        PlayerCommand::SetNotify {
            category: 1,
            on: false,
        },
        None,
    );
    assert_eq!(sim.notify.len(), 1);
    assert!(!sim.notify[0][1]);
}
