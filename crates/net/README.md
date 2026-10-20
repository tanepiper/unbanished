# unbanished-net

Un-Banished multiplayer networking: the protocol between a game and the relay
([unbanished-relay](../../apps/relay/README.md)), and a WebSocket connection to the relay that works
natively and in the browser. The original Banished has no multiplayer; this is the port's own (see
[MULTIPLAYER-PLAN.md](../../docs/MULTIPLAYER-PLAN.md), step 5).

| Item | What it is |
|---|---|
| `ToRelay` | A game to the relay: `Host` a room (with a code or a new one), `Join` one, `Send` data to one player or to every other player |
| `FromRelay` | The relay to a game: `Hosted`, `Joined` (with the player number), `PeerJoined`, `PeerLeft`, `Data` (with the sender), `Error` |
| `encode`, `decode` | One value as one WebSocket binary message (postcard) |
| `Connection` | A connection to a relay: `connect(url)`, `send`, `poll`, `state`. Natively (feature `native`) a background thread holds the socket (tungstenite); in the browser it is the page's `WebSocket` |
| `State` | Connecting, open, or closed (with the reason) |

The relay never reads the data: the game's own messages (its lockstep turns, digests and saves,
`unbanished::gameplay::lockstep::Message`) travel inside `Send` and `Data`.

```rust,ignore
use unbanished_net::{Connection, FromRelay, ToRelay};

let mut relay = Connection::connect("ws://127.0.0.1:7780");
relay.send(ToRelay::Host { room: None });
for message in relay.poll() {
    if let FromRelay::Hosted { room } = message {
        println!("tell the others: {room}");
    }
}
```
