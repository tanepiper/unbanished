//! The Un-Banished multiplayer server: a WebSocket relay with rooms.
//!
//! A game says Hello (its protocol, engine, game data and mods); a host opens a room and gets its
//! code; players join with the code (and the room's password, if it has one), and a game unlike
//! the host is refused with the reason. The relay numbers the players in the order they come
//! (the host is 0), tells everyone in the room who comes and goes, and passes each player's data
//! on to one player or to all the others, without reading it. So nobody forwards a port: the host
//! and the players all connect out to the relay. The protocol and the room rules are in
//! `unbanished-net` (`protocol`, `rooms`).
//!
//! ```text
//! unbanished-server [ADDRESS] [--config FILE]     # default 0.0.0.0:7780
//! ```
//!
//! `GET /health` on the same port answers `ok` with the rooms and games.
//!
//! Not in the original game, which has no multiplayer; see docs/MULTIPLAYER-PLAN.md, step 5.

use std::net::TcpListener;

use unbanished_server::Config;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let config = Config::from_args(&args).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(2);
    });
    let listener = TcpListener::bind(&config.address).unwrap_or_else(|e| {
        eprintln!("{}: {e}", config.address);
        std::process::exit(1);
    });
    log::info!(
        "server (protocol {}) listening on ws://{}",
        unbanished_net::PROTOCOL,
        config.address
    );
    let game = unbanished_server::load_game(&config).unwrap_or_else(|e| {
        eprintln!("simulate: {e}");
        std::process::exit(1);
    });
    if let Some(dir) = &config.simulate {
        log::info!(
            "running each room's game with the game data in {}",
            dir.display()
        );
    }
    unbanished_server::run_with(listener, &config, game);
}
