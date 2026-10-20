//! The Un-Banished multiplayer relay: a WebSocket server with rooms.
//!
//! A host opens a room and gets its code; players join with the code. The relay numbers the
//! players in the order they come (the host is 0), tells everyone in the room who comes and goes,
//! and passes each player's data on to one player or to all the others, without reading it. So
//! nobody forwards a port: the host and the players all connect out to the relay. The protocol is
//! in `banished-net`.
//!
//! ```text
//! banished-relay [ADDRESS]      # default 0.0.0.0:7780
//! ```
//!
//! Not in the original game, which has no multiplayer; see docs/MULTIPLAYER-PLAN.md, step 5.

use std::collections::HashMap;
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use banished_net::{decode, encode, FromRelay, ToRelay};
use tungstenite::Message;

/// Rooms by code: each player's outgoing queue, by player number (None once they left).
type Rooms = Arc<Mutex<HashMap<String, Vec<Option<Sender<FromRelay>>>>>>;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let address = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "0.0.0.0:7780".into());
    let listener = TcpListener::bind(&address).unwrap_or_else(|e| {
        eprintln!("{address}: {e}");
        std::process::exit(1);
    });
    log::info!("relay listening on ws://{address}");
    run(listener);
}

/// Serves every connection to `listener`, each on its own thread.
fn run(listener: TcpListener) {
    let rooms: Rooms = Arc::default();
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let rooms = rooms.clone();
        std::thread::spawn(move || {
            let peer = stream
                .peer_addr()
                .map(|a| a.to_string())
                .unwrap_or_default();
            if let Err(e) = serve(stream, &rooms) {
                log::info!("{peer}: {e}");
            }
        });
    }
}

/// One connection: read what the game sends, write what is queued for it.
fn serve(stream: TcpStream, rooms: &Rooms) -> Result<(), String> {
    let _ = stream.set_nodelay(true);
    let mut socket = tungstenite::accept(stream).map_err(|e| e.to_string())?;
    socket
        .get_ref()
        .set_read_timeout(Some(Duration::from_millis(2)))
        .map_err(|e| e.to_string())?;
    let (queue, outgoing) = channel::<FromRelay>();
    let mut me: Option<(String, u8)> = None;
    let result = (|| -> Result<(), String> {
        loop {
            drain(&mut socket, &outgoing)?;
            match socket.read() {
                Ok(Message::Binary(bytes)) => {
                    let request = decode::<ToRelay>(&bytes)?;
                    handle(request, rooms, &queue, &mut me);
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
    if let Some((room, player)) = me {
        let mut rooms = rooms.lock().unwrap();
        if let Some(players) = rooms.get_mut(&room) {
            players[player as usize] = None;
            for p in players.iter().flatten() {
                let _ = p.send(FromRelay::PeerLeft { player });
            }
            if players.iter().all(Option::is_none) {
                rooms.remove(&room);
                log::info!("room {room} closed");
            }
        }
    }
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

fn handle(
    request: ToRelay,
    rooms: &Rooms,
    queue: &Sender<FromRelay>,
    me: &mut Option<(String, u8)>,
) {
    let mut rooms = rooms.lock().unwrap();
    match request {
        ToRelay::Host { room } => {
            let room = room.unwrap_or_else(|| new_code(&rooms));
            if rooms
                .get(&room)
                .is_some_and(|p| p.iter().any(Option::is_some))
            {
                let message = format!("room {room} is in use");
                let _ = queue.send(FromRelay::Error { message });
                return;
            }
            rooms.insert(room.clone(), vec![Some(queue.clone())]);
            log::info!("room {room} opened");
            let _ = queue.send(FromRelay::Hosted { room: room.clone() });
            *me = Some((room, 0));
        }
        ToRelay::Join { room } => {
            let Some(players) = rooms.get_mut(&room) else {
                let message = format!("no room {room}");
                let _ = queue.send(FromRelay::Error { message });
                return;
            };
            let player = players.len() as u8;
            for p in players.iter().flatten() {
                let _ = p.send(FromRelay::PeerJoined { player });
            }
            players.push(Some(queue.clone()));
            log::info!("room {room}: player {player} joined");
            let _ = queue.send(FromRelay::Joined {
                room: room.clone(),
                player,
            });
            *me = Some((room, player));
        }
        ToRelay::Send { to, data } => {
            let Some((room, from)) = me.as_ref() else {
                return;
            };
            let Some(players) = rooms.get(room) else {
                return;
            };
            for (p, q) in players.iter().enumerate() {
                let wanted = match to {
                    Some(t) => p == t as usize,
                    None => p != *from as usize,
                };
                if let (true, Some(q)) = (wanted, q) {
                    let _ = q.send(FromRelay::Data {
                        from: *from,
                        data: data.clone(),
                    });
                }
            }
        }
    }
}

/// A new room code: six letters, not in use.
fn new_code(rooms: &HashMap<String, Vec<Option<Sender<FromRelay>>>>) -> String {
    let mut seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |d| d.as_nanos() as u64)
        | 1;
    loop {
        let code: String = (0..6)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                // Letters that cannot be mistaken for each other or for digits.
                b"ABCDEFGHJKLMNPQRSTUVWXYZ"[(seed % 24) as usize] as char
            })
            .collect();
        if !rooms.contains_key(&code) {
            return code;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use banished_net::Connection;

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

    #[test]
    fn a_host_and_two_players_pass_data() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || run(listener));
        let mut host = Connection::connect(&url);
        host.send(ToRelay::Host {
            room: Some("TESTER".into()),
        });
        assert_eq!(
            next(&mut host),
            FromRelay::Hosted {
                room: "TESTER".into()
            }
        );
        let mut a = Connection::connect(&url);
        a.send(ToRelay::Join {
            room: "TESTER".into(),
        });
        assert_eq!(
            next(&mut a),
            FromRelay::Joined {
                room: "TESTER".into(),
                player: 1
            }
        );
        assert_eq!(next(&mut host), FromRelay::PeerJoined { player: 1 });
        let mut b = Connection::connect(&url);
        b.send(ToRelay::Join {
            room: "TESTER".into(),
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
        let mut c = Connection::connect(&url);
        c.send(ToRelay::Join {
            room: "NOROOM".into(),
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
