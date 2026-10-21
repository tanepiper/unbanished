//! A relay and its connections in one process, for tests: the same rooms as the relay program
//! ([`crate::rooms`]), and connections that send and poll like a WebSocket `Connection`.
//! Every message is encoded and decoded on the way, as on a socket.

use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::rc::Rc;

use crate::protocol::{decode, encode, FromRelay, ToRelay};
use crate::rooms::{Client, Relay};
use crate::State;

struct Inner {
    relay: Relay,
    inboxes: BTreeMap<Client, VecDeque<Vec<u8>>>,
    closed: BTreeMap<Client, String>,
    next: Client,
}

/// A relay in this process.
#[derive(Clone)]
pub struct MemoryRelay(Rc<RefCell<Inner>>);

impl MemoryRelay {
    /// An empty relay; `seed` starts its room codes.
    pub fn new(seed: u64) -> MemoryRelay {
        MemoryRelay(Rc::new(RefCell::new(Inner {
            relay: Relay::new(seed),
            inboxes: BTreeMap::new(),
            closed: BTreeMap::new(),
            next: 0,
        })))
    }

    /// An empty relay that keeps rooms' towns in `store`.
    pub fn with_store(seed: u64, store: Box<dyn crate::store::Store>) -> MemoryRelay {
        let relay = MemoryRelay::new(seed);
        {
            let mut inner = relay.0.borrow_mut();
            let r = std::mem::replace(&mut inner.relay, Relay::new(seed));
            inner.relay = r.with_store(store);
        }
        relay
    }

    /// A new game's connection to it.
    pub fn connect(&self) -> MemoryConnection {
        let mut inner = self.0.borrow_mut();
        let client = inner.next;
        inner.next += 1;
        inner.relay.connect(client);
        inner.inboxes.insert(client, VecDeque::new());
        MemoryConnection {
            relay: self.0.clone(),
            client,
            state: State::Open,
        }
    }
}

/// A game's connection to a [`MemoryRelay`].
pub struct MemoryConnection {
    relay: Rc<RefCell<Inner>>,
    client: Client,
    state: State,
}

impl MemoryConnection {
    /// Sends a request to the relay.
    pub fn send(&mut self, request: ToRelay) {
        if self.state != State::Open {
            return;
        }
        let request: ToRelay = decode(&encode(&request)).expect("a request round-trips");
        let mut inner = self.relay.borrow_mut();
        let reply = inner.relay.handle(self.client, request);
        for (to, message) in reply.out {
            if let Some(inbox) = inner.inboxes.get_mut(&to) {
                inbox.push_back(encode(&message));
            }
        }
        if reply.close {
            let out = inner.relay.disconnect(self.client);
            for (to, message) in out {
                if let Some(inbox) = inner.inboxes.get_mut(&to) {
                    inbox.push_back(encode(&message));
                }
            }
            inner
                .closed
                .insert(self.client, "the relay closed the connection".into());
        }
    }

    /// The messages that arrived since the last call.
    pub fn poll(&mut self) -> Vec<FromRelay> {
        let mut inner = self.relay.borrow_mut();
        let out = inner
            .inboxes
            .get_mut(&self.client)
            .map(|inbox| inbox.drain(..).collect::<Vec<_>>())
            .unwrap_or_default();
        if let Some(reason) = inner.closed.get(&self.client) {
            self.state = State::Closed(reason.clone());
        }
        out.iter()
            .map(|bytes| decode(bytes).expect("a message round-trips"))
            .collect()
    }

    /// The connection's state.
    pub fn state(&self) -> &State {
        &self.state
    }
}

impl Drop for MemoryConnection {
    fn drop(&mut self) {
        let mut inner = self.relay.borrow_mut();
        let out = inner.relay.disconnect(self.client);
        for (to, message) in out {
            if let Some(inbox) = inner.inboxes.get_mut(&to) {
                inbox.push_back(encode(&message));
            }
        }
        inner.inboxes.remove(&self.client);
    }
}
