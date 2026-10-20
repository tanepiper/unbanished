//! The browser connection: the page's WebSocket, whose events fill a queue the game polls.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use web_sys::{BinaryType, MessageEvent, WebSocket};

use crate::{decode, encode, FromRelay, State, ToRelay};

#[derive(Default)]
struct Shared {
    inbox: VecDeque<FromRelay>,
    state: Option<State>,
}

/// A connection to a relay (`ws://` or `wss://`).
pub struct Connection {
    socket: Option<WebSocket>,
    shared: Rc<RefCell<Shared>>,
    /// Messages given before the socket opened.
    waiting: Vec<ToRelay>,
    state: State,
    _handlers: Vec<Closure<dyn FnMut(web_sys::Event)>>,
}

impl Connection {
    /// Starts connecting; [`Connection::poll`] reports what arrives.
    pub fn connect(url: &str) -> Connection {
        let shared = Rc::new(RefCell::new(Shared::default()));
        let mut handlers = Vec::new();
        let socket = match WebSocket::new(url) {
            Ok(s) => s,
            Err(e) => {
                return Connection {
                    socket: None,
                    shared,
                    waiting: Vec::new(),
                    state: State::Closed(format!("{url}: {e:?}")),
                    _handlers: handlers,
                }
            }
        };
        socket.set_binary_type(BinaryType::Arraybuffer);
        let s = shared.clone();
        let open = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
            s.borrow_mut().state = Some(State::Open);
        });
        socket.set_onopen(Some(open.as_ref().unchecked_ref()));
        handlers.push(open);
        let s = shared.clone();
        let message = Closure::<dyn FnMut(web_sys::Event)>::new(move |e: web_sys::Event| {
            let Ok(e) = e.dyn_into::<MessageEvent>() else {
                return;
            };
            if let Ok(buf) = e.data().dyn_into::<js_sys::ArrayBuffer>() {
                let bytes = js_sys::Uint8Array::new(&buf).to_vec();
                match decode::<FromRelay>(&bytes) {
                    Ok(m) => s.borrow_mut().inbox.push_back(m),
                    Err(e) => log::warn!("relay message: {e}"),
                }
            }
        });
        socket.set_onmessage(Some(message.as_ref().unchecked_ref()));
        handlers.push(message);
        let s = shared.clone();
        let close = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
            s.borrow_mut().state = Some(State::Closed("the connection closed".into()));
        });
        socket.set_onclose(Some(close.as_ref().unchecked_ref()));
        socket.set_onerror(Some(close.as_ref().unchecked_ref()));
        handlers.push(close);
        Connection {
            socket: Some(socket),
            shared,
            waiting: Vec::new(),
            state: State::Connecting,
            _handlers: handlers,
        }
    }

    /// Sends a message to the relay; messages wait until the connection is open.
    pub fn send(&mut self, message: ToRelay) {
        self.waiting.push(message);
        self.flush();
    }

    fn flush(&mut self) {
        if self.state != State::Open {
            return;
        }
        if let Some(socket) = &self.socket {
            for m in self.waiting.drain(..) {
                if let Err(e) = socket.send_with_u8_array(&encode(&m)) {
                    log::warn!("relay send: {e:?}");
                }
            }
        }
    }

    /// What arrived since the last call.
    pub fn poll(&mut self) -> Vec<FromRelay> {
        let (arrived, state) = {
            let mut s = self.shared.borrow_mut();
            (s.inbox.drain(..).collect::<Vec<_>>(), s.state.take())
        };
        if let Some(state) = state {
            self.state = state;
        }
        self.flush();
        arrived
    }

    /// The state of the connection, as of the last [`poll`](Self::poll).
    pub fn state(&self) -> &State {
        &self.state
    }
}

/// Dropped (Cancel, or a game that ends): the page's WebSocket closes, so the relay tells the
/// others that this player left. Without this the socket stays open until the page closes.
impl Drop for Connection {
    fn drop(&mut self) {
        if let Some(socket) = self.socket.take() {
            socket.set_onopen(None);
            socket.set_onmessage(None);
            socket.set_onclose(None);
            socket.set_onerror(None);
            let _ = socket.close();
        }
    }
}
