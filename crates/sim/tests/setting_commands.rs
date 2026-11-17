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
    assert!(sim.events.notify[0]);
    assert!(!sim.events.notify[2]);
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
    assert_eq!(games[0].events.notify, games[1].events.notify);
}

#[test]
fn a_slot_or_category_out_of_range_does_nothing() {
    let archive = Archive::default();
    let mut sim = Simulation::new(1);
    let (limits, notify) = (sim.stock_warnings[0].limits, sim.events.notify);
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
    assert_eq!(sim.events.notify, notify);
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
