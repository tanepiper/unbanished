//! The Un-Banished multiplayer relay: a WebSocket server with rooms.
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
//! unbanished-relay [ADDRESS]      # default 0.0.0.0:7780
//! ```
//!
//! Not in the original game, which has no multiplayer; see docs/MULTIPLAYER-PLAN.md, step 5.

use std::collections::HashMap;
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tungstenite::Message;
use unbanished_net::rooms::{Client, Relay};
use unbanished_net::{decode, encode, FromRelay, ToRelay};

/// The rooms, and each connected game's outgoing queue.
struct Shared {
    relay: Relay,
    queues: HashMap<Client, Sender<FromRelay>>,
}

impl Shared {
    fn deliver(&self, out: Vec<(Client, FromRelay)>) {
        for (to, message) in out {
            if let Some(q) = self.queues.get(&to) {
                let _ = q.send(message);
            }
        }
    }
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let address = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "0.0.0.0:7780".into());
    let listener = TcpListener::bind(&address).unwrap_or_else(|e| {
        eprintln!("{address}: {e}");
        std::process::exit(1);
    });
    log::info!(
        "relay (protocol {}) listening on ws://{address}",
        unbanished_net::PROTOCOL
    );
    run(listener);
}

/// Serves every connection to `listener`, each on its own thread.
fn run(listener: TcpListener) {
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |d| d.as_nanos() as u64);
    let shared = Arc::new(Mutex::new(Shared {
        relay: Relay::new(seed),
        queues: HashMap::new(),
    }));
    let next = Arc::new(AtomicU64::new(0));
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let shared = shared.clone();
        let client = next.fetch_add(1, Ordering::Relaxed);
        std::thread::spawn(move || {
            let peer = stream
                .peer_addr()
                .map(|a| a.to_string())
                .unwrap_or_default();
            if let Err(e) = serve(stream, client, &shared) {
                log::info!("{peer}: {e}");
            }
        });
    }
}

/// One connection: read what the game sends, write what is queued for it.
fn serve(stream: TcpStream, client: Client, shared: &Mutex<Shared>) -> Result<(), String> {
    let _ = stream.set_nodelay(true);
    let mut socket = tungstenite::accept(stream).map_err(|e| e.to_string())?;
    socket
        .get_ref()
        .set_read_timeout(Some(Duration::from_millis(2)))
        .map_err(|e| e.to_string())?;
    let (queue, outgoing) = channel::<FromRelay>();
    {
        let mut s = shared.lock().unwrap();
        s.relay.connect(client);
        s.queues.insert(client, queue);
    }
    let result = (|| -> Result<(), String> {
        loop {
            drain(&mut socket, &outgoing)?;
            match socket.read() {
                Ok(Message::Binary(bytes)) => {
                    let request = decode::<ToRelay>(&bytes)?;
                    let mut s = shared.lock().unwrap();
                    let reply = s.relay.handle(client, request);
                    s.deliver(reply.out);
                    drop(s);
                    if reply.close {
                        drain(&mut socket, &outgoing)?;
                        let _ = socket.close(None);
                        return Ok(());
                    }
                }
                Ok(Message::Close(_)) => return Ok(()),
                Ok(_) => {}
                Err(tungstenite::Error::Io(e))
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(e) => return Err(e.to_string()),
            }
        }
    })();
    // Gone: the others hear of it, and an empty room closes.
    let mut s = shared.lock().unwrap();
    let out = s.relay.disconnect(client);
    s.queues.remove(&client);
    s.deliver(out);
    result
}

fn drain(
    socket: &mut tungstenite::WebSocket<TcpStream>,
    outgoing: &Receiver<FromRelay>,
) -> Result<(), String> {
    loop {
        match outgoing.try_recv() {
            Ok(m) => socket
                .send(Message::Binary(encode(&m).into()))
                .map_err(|e| e.to_string())?,
            Err(TryRecvError::Empty) => return Ok(()),
            Err(TryRecvError::Disconnected) => return Err("queue closed".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use unbanished_net::Connection;

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
        std::thread::spawn(move || run(listener));
        let mut host = greeted(&url);
        host.send(ToRelay::Host {
            room: Some("TESTER".into()),
            password: None,
        });
        assert_eq!(
            next(&mut host),
            FromRelay::Hosted {
                room: "TESTER".into()
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
}
