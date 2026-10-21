//! The ways to the other players: a [`Link`], the relay (`RelayLink`, the `relay` feature), a
//! shared link, and a simulated network in one process for tests ([`SimNet`]).
//!
//! Deviation: not in the original, which has no multiplayer.

use std::cell::RefCell;
use std::rc::Rc;

use crate::lockstep::Message;

/// The way to the other players.
pub trait Link {
    /// To one player, or to every other player.
    fn send(&mut self, to: Option<u8>, message: &Message);
    /// What arrived, with the player it came from.
    fn poll(&mut self) -> Vec<(u8, Message)>;
    /// The players who left since the last call (after everything they sent was polled).
    fn left(&mut self) -> Vec<u8> {
        Vec::new()
    }
}

/// The relay ([`unbanished_net::Connection`]): each game message travels encoded inside the relay's
/// own messages.
#[cfg(feature = "relay")]
pub struct RelayLink {
    connection: unbanished_net::Connection,
    /// The relay's other news (who joined, errors), for the session to read.
    pub events: Vec<unbanished_net::FromRelay>,
    /// Players who left, for [`Link::left`].
    departed: Vec<u8>,
}

#[cfg(feature = "relay")]
impl RelayLink {
    pub fn new(connection: unbanished_net::Connection) -> RelayLink {
        RelayLink {
            connection,
            events: Vec::new(),
            departed: Vec::new(),
        }
    }

    pub fn state(&self) -> &unbanished_net::State {
        self.connection.state()
    }

    pub fn request(&mut self, request: unbanished_net::ToRelay) {
        self.connection.send(request);
    }
}

#[cfg(feature = "relay")]
impl Link for RelayLink {
    fn send(&mut self, to: Option<u8>, message: &Message) {
        let data = unbanished_net::encode(message);
        self.connection
            .send(unbanished_net::ToRelay::Send { to, data });
    }

    fn poll(&mut self) -> Vec<(u8, Message)> {
        let mut out = Vec::new();
        for m in self.connection.poll() {
            match m {
                unbanished_net::FromRelay::Data { from, data } => {
                    match unbanished_net::decode::<Message>(&data) {
                        Ok(message) => out.push((from, message)),
                        Err(e) => log::warn!("player {from}: {e}"),
                    }
                }
                unbanished_net::FromRelay::PeerLeft { player } => {
                    self.departed.push(player);
                    self.events
                        .push(unbanished_net::FromRelay::PeerLeft { player });
                }
                other => self.events.push(other),
            }
        }
        out
    }

    fn left(&mut self) -> Vec<u8> {
        // In a game nobody else reads the news: it is dropped here.
        self.events.clear();
        std::mem::take(&mut self.departed)
    }
}

/// A link that can be shared: the session reads the relay's news while the peer owns the link.
pub struct Shared<L: Link>(pub Rc<RefCell<L>>);

impl<L: Link> Link for Shared<L> {
    fn send(&mut self, to: Option<u8>, message: &Message) {
        self.0.borrow_mut().send(to, message);
    }

    fn poll(&mut self) -> Vec<(u8, Message)> {
        self.0.borrow_mut().poll()
    }

    fn left(&mut self) -> Vec<u8> {
        self.0.borrow_mut().left()
    }
}

/// A simulated network for a test in one process: a message arrives 1 to `1 + jitter` ticks
/// after it is sent.
pub struct SimNet {
    pub tick: u64,
    jitter: u64,
    seed: u64,
    queue: Vec<(u64, u8, u8, Message)>,
}

impl SimNet {
    pub fn new(jitter: u64, seed: u64) -> Rc<RefCell<SimNet>> {
        Rc::new(RefCell::new(SimNet {
            tick: 0,
            jitter,
            seed: seed | 1,
            queue: Vec::new(),
        }))
    }

    /// The network's random numbers (not the game's).
    pub fn random(&mut self, n: u64) -> u64 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
        if n == 0 {
            0
        } else {
            self.seed % n
        }
    }

    pub fn in_flight(&self) -> bool {
        !self.queue.is_empty()
    }
}

/// One player's end of a [`SimNet`].
pub struct SimLink {
    pub net: Rc<RefCell<SimNet>>,
    pub player: u8,
    pub players: u8,
}

impl Link for SimLink {
    fn send(&mut self, to: Option<u8>, message: &Message) {
        let mut net = self.net.borrow_mut();
        for q in 0..self.players {
            let wanted = to.map_or(q != self.player, |t| t == q);
            if wanted {
                let jitter = net.jitter;
                let at = net.tick + 1 + net.random(jitter + 1);
                net.queue.push((at, self.player, q, message.clone()));
            }
        }
    }

    fn poll(&mut self) -> Vec<(u8, Message)> {
        let mut net = self.net.borrow_mut();
        let tick = net.tick;
        let mut out = Vec::new();
        let me = self.player;
        net.queue.retain(|(at, from, to, m)| {
            let due = *to == me && *at <= tick;
            if due {
                out.push((*from, m.clone()));
            }
            !due
        });
        out
    }
}
