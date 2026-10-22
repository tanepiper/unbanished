//! The headless lockstep and network tests.

use super::*;

/// `--lockstep-test SAVE RECORDING [PLAYERS] [DELAY] [JITTER] [STALL] [SEED] [CHECK]
/// [CORRUPT]`: plays a recording's commands, split between PLAYERS (default 3), in lockstep
/// games joined by a simulated network (`gameplay::lockstep::simulate`), and compares their
/// digests. The host checks digests every CHECK updates (default 600, 0 none) and resyncs
/// the players from its save when they differ; CORRUPT `PLAYER:UPDATE` scrambles one
/// player's game there, to test that. Exits with 1 when the players disagree at the end.
#[cfg(feature = "multiplayer")]
pub fn lockstep_test(args: &[String]) {
    let (Some(save), Some(recording)) = (args.get(1), args.get(2)) else {
        eprintln!(
            "--lockstep-test SAVE RECORDING [PLAYERS] [DELAY] [JITTER] [STALL] [SEED] [CHECK] [CORRUPT]"
        );
        std::process::exit(2);
    };
    let number =
        |i: usize, default: u64| args.get(i).and_then(|s| s.parse().ok()).unwrap_or(default);
    let sim = crate::gameplay::network::Simulation {
        players: number(3, 3) as u8,
        delay: number(4, 6),
        jitter: number(5, 8),
        stall: number(6, 20),
        every: 60,
        check: number(8, 600),
        corrupt: args.get(9).and_then(|s| {
            let (p, u) = s.split_once(':')?;
            Some((p.parse().ok()?, u.parse().ok()?))
        }),
        seed: number(7, 1),
    };
    let bytes = std::fs::read(save).expect("read the save");
    let text = std::fs::read_to_string(recording).expect("read the recording");
    let dir = data_dir(args);
    let found = crate::mods::find(&dir);
    let mut game = Game::with_mods(
        read_packages(&dir),
        found,
        Some(dir),
        crate::settings::Settings::default(),
    )
    .expect("game data");
    game.audio.set_master_volume(0.0);
    let start = std::time::Instant::now();
    match crate::gameplay::network::simulate(&mut game, &bytes, &text, sim, 0) {
        Ok(r) => {
            for d in &r.digests[0] {
                println!("{d}");
            }
            log::info!("simulated in {:.1} s", start.elapsed().as_secs_f32());
            println!(
                "{sim:?}: {} ticks, players at most {} updates apart",
                r.ticks, r.lead
            );
            println!(
                "desyncs found at updates {:?}, resyncs at {:?}",
                r.desyncs, r.resyncs
            );
            if r.recorded > 0 {
                match r.recorded_mismatch {
                    Some((a, b)) => println!("DIFFERENT from the recording: {a} / {b}"),
                    None => println!("{} recorded digests matched", r.recorded),
                }
            }
            match r.mismatch {
                Some((p, a, b)) => {
                    println!("PLAYERS DISAGREE: player {p}");
                    println!("  player 0 {a}");
                    println!("  player {p} {b}");
                    std::process::exit(1);
                }
                None if r.recorded_mismatch.is_some() => std::process::exit(1),
                None => println!(
                    "identical: {} players agree on {} digests",
                    sim.players,
                    r.digests[0].len()
                ),
            }
        }
        Err(e) => {
            eprintln!("--lockstep-test: {e}");
            std::process::exit(2);
        }
    }
}

/// `--net-test RELAY host|join ROOM SAVE RECORDING PLAYERS [DELAY] [CHECK] [CORRUPT]`: one
/// player of a networked lockstep game through a relay (`unbanished-server`). The host opens
/// ROOM, waits for PLAYERS, and starts the game from SAVE (joining players get it from the
/// host and ignore SAVE); each plays its share of RECORDING's commands and prints its
/// digests. CORRUPT `UPDATE` scrambles this player's game there. tools/compat/network.sh runs
/// several and compares them.
#[cfg(feature = "multiplayer")]
pub fn net_test(args: &[String]) {
    use crate::gameplay::network::{NetPeer, RelayLink, Script, Shared};
    let usage = "--net-test RELAY host|join ROOM SAVE RECORDING PLAYERS [DELAY] [CHECK] [CORRUPT]";
    let (Some(relay), Some(role), Some(room), Some(save), Some(recording), Some(players)) = (
        args.get(1),
        args.get(2),
        args.get(3),
        args.get(4),
        args.get(5),
        args.get(6).and_then(|s| s.parse::<u8>().ok()),
    ) else {
        eprintln!("{usage}");
        std::process::exit(2);
    };
    let number =
        |i: usize, default: u64| args.get(i).and_then(|s| s.parse().ok()).unwrap_or(default);
    let (delay, check) = (number(7, 6), number(8, 120));
    let corrupt = args.get(9).and_then(|s| s.parse::<u64>().ok());
    // `--leave-at N`: this player leaves the game at update N. `--resume`: the host goes on
    // from the room's town kept by the server, if it has one, instead of SAVE.
    let leave_at = args
        .windows(2)
        .find(|w| w[0] == "--leave-at")
        .and_then(|w| w[1].parse::<u64>().ok());
    let resume = args.iter().any(|a| a == "--resume");
    let host = role == "host";
    let text = std::fs::read_to_string(recording).expect("read the recording");
    let dir = data_dir(args);
    let found = crate::mods::find(&dir);
    let mut game = Game::with_mods(
        read_packages(&dir),
        found,
        Some(dir),
        crate::settings::Settings::default(),
    )
    .expect("game data");
    game.audio.set_master_volume(0.0);
    let link = std::rc::Rc::new(std::cell::RefCell::new(RelayLink::new(
        unbanished_net::Connection::connect(relay),
        crate::gameplay::network::hello(&game.archive),
    )));
    let request = match host {
        true => unbanished_net::ToRelay::Host {
            room: Some(room.clone()),
            password: None,
        },
        false => unbanished_net::ToRelay::Join {
            room: room.clone(),
            password: None,
        },
    };
    link.borrow_mut().request(request);
    // Who this player is, and (the host) everybody there.
    let wait = |what: &str, done: &mut dyn FnMut(&unbanished_net::FromRelay) -> bool| {
        let start = std::time::Instant::now();
        loop {
            let _ = crate::gameplay::network::Link::poll(&mut *link.borrow_mut());
            let events = std::mem::take(&mut link.borrow_mut().events);
            for e in &events {
                if let unbanished_net::FromRelay::Error { message } = e {
                    eprintln!("relay: {message}");
                    std::process::exit(2);
                }
                if done(e) {
                    return;
                }
            }
            if let unbanished_net::State::Closed(e) = link.borrow().state() {
                eprintln!("relay: {e}");
                std::process::exit(2);
            }
            if start.elapsed().as_secs() > 120 {
                eprintln!("waited too long for {what}");
                std::process::exit(2);
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    };
    let mut me = 0u8;
    let mut kept = None;
    wait("the room", &mut |e| match e {
        unbanished_net::FromRelay::Hosted { kept: k, .. } => {
            kept = *k;
            true
        }
        unbanished_net::FromRelay::Joined { player, .. } => {
            me = *player;
            true
        }
        _ => false,
    });
    log::info!("player {me} in room {room}");
    let mut peer = NetPeer::new(me, Box::new(Shared(link.clone())), check);
    let mut script = Script::new(&text, players as usize, delay, false).expect("the recording");
    let last = script.last + delay + 1;
    let start = std::time::Instant::now();
    let outcome = game.with_ctx(|_, ctx| -> Result<Vec<String>, String> {
        if host {
            let mut joined = 1;
            wait("the players", &mut |e| {
                if let unbanished_net::FromRelay::PeerJoined { .. } = e {
                    joined += 1;
                }
                joined >= players
            });
            let bytes = match kept.filter(|_| resume) {
                Some(update) => {
                    link.borrow_mut().request(unbanished_net::ToRelay::Fetch);
                    let mut town = Vec::new();
                    wait("the kept town", &mut |e| match e {
                        unbanished_net::FromRelay::Kept { bytes, .. } => {
                            town = bytes.clone();
                            true
                        }
                        _ => false,
                    });
                    log::info!("resuming the room's town kept at update {update}");
                    town
                }
                None => std::fs::read(save).map_err(|e| e.to_string())?,
            };
            peer.host_start(ctx, bytes, players, delay)?;
        }
        let mut lines = Vec::new();
        let mut corrupt = corrupt;
        loop {
            let arrived = peer.poll(ctx)?;
            let Some(g) = peer.game.as_mut() else {
                if !arrived {
                    std::thread::sleep(std::time::Duration::from_millis(2));
                }
                continue;
            };
            if g.updates() > last {
                break;
            }
            script.give(me as usize, g, ctx);
            let Some(update) = peer.begin(ctx) else {
                if !arrived {
                    std::thread::sleep(std::time::Duration::from_micros(500));
                }
                continue;
            };
            if leave_at.is_some_and(|at| update >= at) {
                log::info!("player {me} leaves at update {update}");
                break;
            }
            if corrupt.is_some_and(|at| update >= at) {
                peer.game.as_mut().unwrap().scramble();
                corrupt = None;
            }
            peer.boundary(ctx, update)?;
            if update % 60 == 0 {
                // Printed at once, so that a run cut short can be compared too; after a
                // resync an update can come again, and its last line counts.
                let d = peer.game.as_ref().unwrap().state_digest(ctx, update)?;
                println!("{d}");
                lines.push(d.to_string());
            }
            peer.update(ctx);
        }
        // Let the last messages go out before the connection closes.
        let linger = std::time::Instant::now();
        while linger.elapsed().as_millis() < 1500 {
            peer.poll(ctx)?;
            peer.begin(ctx);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        Ok(lines)
    });
    match outcome {
        Ok(_) => {
            println!("desyncs {:?} resyncs {:?}", peer.desyncs(), peer.resyncs());
            log::info!(
                "player {me}: {} updates in {:.1} s",
                last,
                start.elapsed().as_secs_f32()
            );
        }
        Err(e) => {
            eprintln!("--net-test: {e}");
            std::process::exit(2);
        }
    }
}
