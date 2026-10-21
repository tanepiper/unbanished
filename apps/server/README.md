# unbanished-server

The Un-Banished multiplayer server. It is a WebSocket relay with rooms: it holds no game
data and runs no game, the players bring their own copy of Banished. It checks that the players
still play the same game, lets players join a game that has started, and can keep each room's
town between games ([ARCHITECTURE-PLAN.md](../../docs/ARCHITECTURE-PLAN.md), phase D).

A game first says Hello: the protocol version, its engine version, and fingerprints of its game
data and mods. A server of another protocol version refuses it with the reason. A host opens a
room (with a password if it wants) and gets its code; players join with the code. A player whose
engine, data or mods differ from the host's is refused, and told which. The server numbers the
players in the order they come (the host is 0), tells everyone in the room who comes and goes,
and passes each player's data on to one player or to all the others, without reading it. So
nobody forwards a port: the host and the players all connect out to the server. The protocol and
the room rules are in [unbanished-net](../../crates/net/README.md).

## Run it

```bash
cargo run --release -p unbanished-server                       # listens on 0.0.0.0:7780
cargo run --release -p unbanished-server -- 127.0.0.1:7780     # only games on this computer
cargo run --release -p unbanished-server -- --config server.conf
```

A settings file has one `key = value` per line; `#` starts a comment. An address on the command
line replaces the file's.

```text
address = 0.0.0.0:7780       # where to listen
max_rooms = 100              # the most rooms open at once (the default)
rooms = /var/lib/unbanished  # keep each room's town here (without it, nothing is kept)
```

With `rooms`, the server keeps each room's town: the host's game sends it a save when the game
starts, at every resync or new player, and every minute of play. When the last player leaves, the
town stays in the folder (`CODE.sav`, `CODE.update`). A host who opens a room with the same code
later is told it has a town, and **Start** in the Multiplayer window goes on from it instead of
asking for a save. Only codes of letters and digits (at most 16) are kept. In a container, give
the folder a volume: the image has `/rooms` for it, so `docker run -v unbanished-rooms:/rooms`
with `rooms = /rooms` in the settings file.

`GET /health` on the same port is a plain HTTP request that answers `ok rooms=N games=N`. Use it
for a container's health check or a proxy's.

```bash
curl http://127.0.0.1:7780/health
```

## In the desktop game

The server is also a library: the desktop game runs it on a thread to host a game itself
(`unbanished_server::spawn`). A host who opens a room at `ws://127.0.0.1:PORT` with no server
running there gets one, listening on every network of the computer. See the
[multiplayer guide](../../docs/guide/MULTIPLAYER.md).

## In a container

The image holds only the server. Build it from the repository's root; the `.dockerignore` there
keeps the game's files, saves and analysis out of the build.

```bash
docker build -f apps/server/Dockerfile -t unbanished-server .
docker run -p 7780:7780 unbanished-server
```

## Players in the browser: TLS

The server speaks plain `ws://`. The desktop game can use that directly. A page served over
`https://`, like the published Play page, may only open `wss://`. So a server for browser players
needs TLS in front of it, from a reverse proxy. With [Caddy](https://caddyserver.com), which gets
a certificate by itself, this `Caddyfile` is enough:

```text
play.example.org {
    reverse_proxy 127.0.0.1:7780
}
```

Players then enter `wss://play.example.org` as the server in the Multiplayer window.

## The code

| Source | What it holds |
|---|---|
| `src/lib.rs` | The listener, one thread per connection, and `spawn` (the server on a thread, for the desktop game); tests of a host and two players over sockets, and of the health check |
| `src/main.rs` | The program: the settings, then `run` |
| `src/serve.rs` | One game's connection: its requests go to the rooms, and the rooms' messages come back |
| `src/config.rs` | The command line and the settings file |
| `src/health.rs` | `GET /health` |
| `Dockerfile` | The container image |

The rooms themselves are `unbanished_net::rooms::Relay`, a state machine with no sockets that the
tests also run in one process. `tools/compat/network.sh` and `tools/compat/multiplayer.sh` start
a server and play several games through it. See
[MULTIPLAYER-PLAN.md](../../docs/MULTIPLAYER-PLAN.md), step 5.
