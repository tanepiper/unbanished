//! The native connection: a background thread holds the WebSocket and passes messages through
//! channels, so the game never waits on the network.

use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::time::Duration;

use crate::{decode, encode, FromRelay, State, ToRelay};

/// A connection to a relay (`ws://host:port`).
pub struct Connection {
    out: Sender<ToRelay>,
    inbox: Receiver<Result<FromRelay, String>>,
    state: State,
}

impl Connection {
    /// Starts connecting; [`Connection::poll`] reports what arrives.
    pub fn connect(url: &str) -> Connection {
        let (out, outgoing) = channel::<ToRelay>();
        let (incoming, inbox) = channel();
        let url = url.to_string();
        std::thread::spawn(move || {
            if let Err(e) = run(&url, outgoing, &incoming) {
                let _ = incoming.send(Err(e));
            }
        });
        Connection {
            out,
            inbox,
            state: State::Connecting,
        }
    }

    /// Sends a message to the relay (dropped once the connection is closed).
    pub fn send(&mut self, message: ToRelay) {
        let _ = self.out.send(message);
    }

    /// What arrived since the last call.
    pub fn poll(&mut self) -> Vec<FromRelay> {
        let mut out = Vec::new();
        loop {
            match self.inbox.try_recv() {
                Ok(Ok(m)) => {
                    self.state = State::Open;
                    out.push(m);
                }
                Ok(Err(e)) => self.state = State::Closed(e),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    if !matches!(self.state, State::Closed(_)) {
                        self.state = State::Closed("disconnected".into());
                    }
                    break;
                }
            }
        }
        out
    }

    /// The state of the connection, as of the last [`poll`](Self::poll).
    pub fn state(&self) -> &State {
        &self.state
    }
}

fn run(
    url: &str,
    outgoing: Receiver<ToRelay>,
    incoming: &Sender<Result<FromRelay, String>>,
) -> Result<(), String> {
    use tungstenite::{stream::MaybeTlsStream, Message};
    let (mut socket, _) = tungstenite::connect(url).map_err(|e| format!("{url}: {e}"))?;
    if let MaybeTlsStream::Plain(s) = socket.get_ref() {
        s.set_read_timeout(Some(Duration::from_millis(2)))
            .map_err(|e| e.to_string())?;
        let _ = s.set_nodelay(true);
    }
    loop {
        // Send everything queued, then read what is there.
        loop {
            match outgoing.try_recv() {
                Ok(m) => socket
                    .send(Message::Binary(encode(&m).into()))
                    .map_err(|e| e.to_string())?,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    let _ = socket.close(None);
                    return Ok(());
                }
            }
        }
        match socket.read() {
            Ok(Message::Binary(bytes)) => {
                let m = decode::<FromRelay>(&bytes)?;
                if incoming.send(Ok(m)).is_err() {
                    return Ok(());
                }
            }
            Ok(Message::Close(_)) => return Err("the relay closed the connection".into()),
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(e) => return Err(e.to_string()),
        }
    }
}
