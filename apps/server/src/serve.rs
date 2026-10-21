//! One game's connection: what it sends goes to the rooms, and what the rooms send it goes back.

use std::collections::HashMap;
use std::net::TcpStream;
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tungstenite::Message;
use unbanished_net::rooms::{Client, Relay};
use unbanished_net::{decode, encode, FromRelay, ToRelay};

/// The rooms, each connected game's outgoing queue, and the server's own games.
pub struct Shared {
    pub relay: Relay,
    pub queues: HashMap<Client, Sender<FromRelay>>,
    /// The operator's game data (`simulate`), to run rooms' games with.
    pub game: Option<crate::watch::GameData>,
    /// The rooms whose game the server runs, by room code.
    pub watchers: HashMap<String, Sender<crate::watch::In>>,
}

impl Shared {
    pub fn deliver(&self, out: Vec<(Client, FromRelay)>) {
        for (to, message) in out {
            if let Some(q) = self.queues.get(&to) {
                let _ = q.send(message);
            }
        }
    }
}

/// One connection: read what the game sends, write what is queued for it.
pub fn serve(stream: TcpStream, client: Client, shared: &Arc<Mutex<Shared>>) -> Result<(), String> {
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
                    let at = s.relay.room_of(client);
                    // What the server's own game of the room needs to hear.
                    let heard = match (&request, &at) {
                        (ToRelay::Send { to: None, data }, Some(_)) => Some(data.clone()),
                        _ => None,
                    };
                    let leaving = matches!(request, ToRelay::Leave);
                    let reply = s.relay.handle(client, request);
                    s.deliver(reply.out);
                    if let (Some(data), Some((room, _))) = (heard, &at) {
                        crate::watch::heard(&mut s, shared, room, &data);
                    }
                    if let (true, Some((room, player))) = (leaving, &at) {
                        crate::watch::left(&mut s, room, *player);
                    }
                    crate::watch::saves_wanted(&mut s);
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
    let at = s.relay.room_of(client);
    let out = s.relay.disconnect(client);
    s.queues.remove(&client);
    s.deliver(out);
    if let Some((room, player)) = at {
        crate::watch::left(&mut s, &room, player);
    }
    crate::watch::saves_wanted(&mut s);
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
