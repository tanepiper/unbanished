//! One game's connection: what it sends goes to the rooms, and what the rooms send it goes back.

use std::collections::HashMap;
use std::net::TcpStream;
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::Mutex;
use std::time::Duration;

use tungstenite::Message;
use unbanished_net::rooms::{Client, Relay};
use unbanished_net::{decode, encode, FromRelay, ToRelay};

/// The rooms, and each connected game's outgoing queue.
pub struct Shared {
    pub relay: Relay,
    pub queues: HashMap<Client, Sender<FromRelay>>,
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

/// One connection: read what the game sends, write what is queued for it.
pub fn serve(stream: TcpStream, client: Client, shared: &Mutex<Shared>) -> Result<(), String> {
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
