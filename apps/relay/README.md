# unbanished-relay

The Un-Banished multiplayer relay: a small WebSocket server with rooms. A host opens a room and
gets its code; players join with the code. The relay numbers the players in the order they come
(the host is 0), tells everyone in the room who comes and goes, and passes each player's data on to
one player or to all the others, without reading it. So nobody forwards a port: the host and the
players all connect out to the relay. The protocol is in [unbanished-net](../../crates/net/README.md).

```bash
cargo run --release -p unbanished-relay -- 0.0.0.0:7780     # the default address
```

Each connection has a thread; a room closes when its last player leaves. The relay speaks plain
`ws://`. A page served over `https://` (the published site) may only open `wss://`, so a public relay
needs TLS in front of it (a reverse proxy such as Caddy or nginx).

| Source | What it holds |
|---|---|
| `src/main.rs` | The listener, one thread per connection, the rooms, room codes, and a test of a host and two players |

`tools/compat/network.sh` starts a relay and plays several games through it. See
[MULTIPLAYER-PLAN.md](../../docs/MULTIPLAYER-PLAN.md), step 5.
