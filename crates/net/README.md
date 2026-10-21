# unbanished-net

Un-Banished multiplayer networking: the protocol between a game and the relay
([unbanished-server](../../apps/server/README.md)), and a WebSocket connection to the relay that works
natively and in the browser. The original Banished has no multiplayer; this is the port's own (see
[MULTIPLAYER-PLAN.md](../../docs/MULTIPLAYER-PLAN.md), step 5).

Protocol version 1. A game's first message is `Hello`; the relay answers `Welcome`, or an
`Error` with the reason when the versions differ.

| Item | What it is |
|---|---|
| `PROTOCOL`, `Hello` | The protocol version, and who a game is: the protocol, its engine version, and fingerprints of its game data and mods (`Archive::fingerprints`). `Hello::differs` says why two games cannot share a room |
| `ToRelay` | A game to the relay: `Hello`, `ListRooms`, `Host` a room (a code or a new one, and a password or none), `Join` one (with its password), `Leave`, `Send` data to one player or to every other player, `Ping` |
| `FromRelay` | The relay to a game: `Welcome`, `Rooms`, `Hosted`, `Joined` (with the player number), `PeerJoined`, `PeerLeft`, `Data` (with the sender), `Pong`, `Error` (with the reason) |
| `encode`, `decode` | One value as one WebSocket binary message (postcard). A cut-off or unknown message decodes to an error |
| `rooms::Relay` | The relay's rules with no sockets: a request goes in, the messages for each game come out. The relay program and the tests use it |
| `memory::MemoryRelay` | A relay and its connections in one process, for tests |
| `Connection` | A connection to a relay: `connect(url)`, `send`, `poll`, `state`. Natively (feature `native`) a background thread holds the socket (tungstenite); in the browser it is the page's `WebSocket` |
| `State` | Connecting, open, or closed (with the reason) |

The relay never reads the data: the game's own messages (its lockstep turns, digests and saves,
`unbanished_session::lockstep::Message`) travel inside `Send` and `Data`.

```rust,ignore
use unbanished_net::{Connection, FromRelay, Hello, ToRelay};

let mut relay = Connection::connect("ws://127.0.0.1:7780");
relay.send(ToRelay::Hello(Hello::new("Un-Banished 0.4.0", data, mods)));
relay.send(ToRelay::Host { room: None, password: None });
for message in relay.poll() {
    if let FromRelay::Hosted { room } = message {
        println!("tell the others: {room}");
    }
}
```
