//! The Un-Banished multiplayer server: a WebSocket relay with rooms. The program
//! `unbanished-server` runs it; the desktop game runs it on a thread to host a game itself
//! ([`spawn`]).
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

pub mod config;
mod health;
mod serve;

use std::net::TcpListener;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use unbanished_net::rooms::Relay;

pub use config::Config;
use serve::Shared;

/// Listens at `config.address` and serves on a thread of its own: the desktop game hosting a
/// game. The address it listens at.
pub fn spawn(config: Config) -> std::io::Result<std::net::SocketAddr> {
    let listener = TcpListener::bind(&config.address)?;
    let address = listener.local_addr()?;
    std::thread::Builder::new()
        .name("unbanished-server".into())
        .spawn(move || run(listener, &config))?;
    log::info!(
        "server (protocol {}) on ws://{address}",
        unbanished_net::PROTOCOL
    );
    Ok(address)
}

/// Serves every connection to `listener`, each on its own thread.
pub fn run(listener: TcpListener, config: &Config) {
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |d| d.as_nanos() as u64);
    let mut relay = Relay::new(seed);
    relay.max_rooms = config.max_rooms;
    if let Some(dir) = &config.rooms {
        log::info!("keeping rooms' towns in {}", dir.display());
        relay = relay.with_store(Box::new(unbanished_net::store::FolderStore(dir.clone())));
    }
    let shared = Arc::new(Mutex::new(Shared {
        relay,
        queues: Default::default(),
    }));
    let next = Arc::new(AtomicU64::new(0));
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let shared = shared.clone();
        let client = next.fetch_add(1, Ordering::Relaxed);
        std::thread::spawn(move || {
            let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(5)));
            if health::is_check(&stream) {
                let s = shared.lock().unwrap();
                let (rooms, games) = (s.relay.room_count(), s.relay.game_count());
                drop(s);
                health::answer(stream, rooms, games);
                return;
            }
            let peer = stream
                .peer_addr()
                .map(|a| a.to_string())
                .unwrap_or_default();
            if let Err(e) = serve::serve(stream, client, &shared) {
                log::info!("{peer}: {e}");
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use unbanished_net::{Connection, FromRelay, ToRelay};

    /// Waits for the next message on `c`.
    fn next(c: &mut Connection) -> FromRelay {
        for _ in 0..2000 {
            if let Some(m) = c.poll().into_iter().next() {
                return m;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        panic!("nothing arrived: {:?}", c.state());
    }

    /// A connection that has said Hello and been welcomed.
    fn greeted(url: &str) -> Connection {
        let mut c = Connection::connect(url);
        c.send(ToRelay::Hello(unbanished_net::Hello::new("test", 1, 2)));
        assert!(matches!(next(&mut c), FromRelay::Welcome { .. }));
        c
    }

    #[test]
    fn a_host_and_two_players_pass_data() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || run(listener, &Config::default()));
        let mut host = greeted(&url);
        host.send(ToRelay::Host {
            room: Some("TESTER".into()),
            password: None,
        });
        assert_eq!(
            next(&mut host),
            FromRelay::Hosted {
                room: "TESTER".into(),
                keeps: false,
                kept: None
            }
        );
        let mut a = greeted(&url);
        a.send(ToRelay::Join {
            room: "TESTER".into(),
            password: None,
        });
        assert_eq!(
            next(&mut a),
            FromRelay::Joined {
                room: "TESTER".into(),
                player: 1
            }
        );
        assert_eq!(next(&mut host), FromRelay::PeerJoined { player: 1 });
        let mut b = greeted(&url);
        b.send(ToRelay::Join {
            room: "TESTER".into(),
            password: None,
        });
        assert!(matches!(next(&mut b), FromRelay::Joined { player: 2, .. }));
        // To everyone else, and to one player; a big message too.
        host.send(ToRelay::Send {
            to: None,
            data: vec![1; 3 << 20],
        });
        for c in [&mut a, &mut b] {
            loop {
                match next(c) {
                    FromRelay::Data { from: 0, data } => {
                        assert_eq!(data.len(), 3 << 20);
                        break;
                    }
                    FromRelay::PeerJoined { .. } => {}
                    other => panic!("{other:?}"),
                }
            }
        }
        b.send(ToRelay::Send {
            to: Some(1),
            data: vec![9],
        });
        assert_eq!(
            next(&mut a),
            FromRelay::Data {
                from: 2,
                data: vec![9]
            }
        );
        // A room that does not exist.
        let mut c = greeted(&url);
        c.send(ToRelay::Join {
            room: "NOROOM".into(),
            password: None,
        });
        assert!(matches!(next(&mut c), FromRelay::Error { .. }));
        // A player leaving.
        drop(a);
        loop {
            match next(&mut host) {
                FromRelay::PeerLeft { player: 1 } => break,
                FromRelay::PeerJoined { .. } => {}
                other => panic!("{other:?}"),
            }
        }
    }

    #[test]
    fn a_health_check_answers_ok() {
        use std::io::{Read, Write};
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        std::thread::spawn(move || run(listener, &Config::default()));
        let mut s = std::net::TcpStream::connect(address).unwrap();
        s.write_all(b"GET /health HTTP/1.1\r\nHost: x\r\n\r\n")
            .unwrap();
        let mut answer = String::new();
        s.read_to_string(&mut answer).unwrap();
        assert!(answer.starts_with("HTTP/1.1 200 OK"), "{answer}");
        assert!(answer.ends_with("ok rooms=0 games=0\n"), "{answer}");
    }
}
