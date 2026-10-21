# How Un-Banished is built

This page explains how the code fits together: what each part does, which part may use which,
and how one update of the game moves through them. It is for anyone who wants to change the code
or build something on it. The plan that led here, with every step and how it was checked, is
[ARCHITECTURE-PLAN.md](ARCHITECTURE-PLAN.md).

## The idea: a core, and everything else around it

Un-Banished is two things.

- The **core** is Banished 1.0.7, rebuilt one to one: the town's rules and numbers, its saves,
  and the original interface, drawing and sound. It must always behave exactly as the original.
- An **extension** is anything Un-Banished adds that the original does not have: multiplayer, the
  server, recordings, the probe, the mod sites, the cinematic camera. Each one can be left out.
  None may change the core's results.

The code follows a pattern close to model–view–controller:

| Part | In Un-Banished | Where |
|---|---|---|
| Model | The **simulation**: the town and everything that decides what happens in it. It changes only through player commands and its own update. | `crates/sim` and below |
| View | The **presentation**: drawing, sound, the interface. It reads the simulation and turns the player's input into commands. | `apps/game` |
| Controller | The **session**: it decides when a command reaches the simulation. At once in single player, at the same update on every computer in multiplayer. | `crates/session` |

## The layers

Each layer uses only the layers below it.

```text
Server         apps/server      unbanished-server   rooms, checks, kept towns, the server's own game
Protocol       crates/net       unbanished-net      what a game and the server say, and the rooms' rules
Session        crates/session   unbanished-session  when commands are carried out; lockstep; recordings
Presentation   apps/game        unbanished-app      the window, drawing, sound, interface, input
Core           crates/sim       unbanished-sim      the running town: Simulation, the town, saves, commands
               crates/game      unbanished-game     the game's rules (citizens, jobs, buildings, nature)
               crates/terrain, crates/path, crates/core, crates/formats
```

| Crate | Layer | What it holds |
|---|---|---|
| `unbanished-core` | core | Exact maths (the C runtime's float functions), the game's random numbers, noise, game time, the calendar |
| `unbanished-formats` | core | Readers and writers for the game's files: packages, resources, meshes, textures, sounds, dialogs, saves |
| `unbanished-terrain`, `unbanished-path` | core | Map generation, the terrain mesh, ground queries; the pathfinder and the sector graph |
| `unbanished-game` | core | The game's rules, each checked against the original |
| `unbanished-sim` | core | The running town: `Simulation` (the town, the map's heights, the calendar, the random stream, the environment, the event log, the statistics), player commands, the tools' core, loading and writing saves, achievements |
| `unbanished-ui`, `unbanished-gfx`, `unbanished-audio`, `unbanished-wma` | presentation | The interface system, rendering, sound playback, the WMA decoder |
| `unbanished-app` | presentation | The game program: the window, the original's screens and windows, drawing the town, sound, input |
| `unbanished-session` | session | `Session` (single player, lockstep), lockstep turns and messages, links to the other players, digests, recordings, and `Watcher` (a town run by the server) |
| `unbanished-net` | protocol | The protocol, the rooms' rules (`rooms::Relay`), the checks of the players' games, kept towns, a WebSocket connection |
| `unbanished-server` | server | The server program and library: connections, health check, settings, the server's own games |
| `unbanished-mods`, `unbanished-nexus`, `unbanished-banishedinfo` | presentation (extensions) | The mod list; the Nexus Mods and banishedinfo.com sites |
| `unbanished-hooks` | core (extension) | Rule hooks from WebAssembly plugins, behind the core's `RuleHooks` door |

## The rules

1. **The core stands alone.** `unbanished-sim` depends on no drawing, window, interface, sound
   or network crate. The `checks` test suite fails if it does. It builds for the browser too, and
   runs with no GPU.
2. **The core never depends on an extension.** Extensions reach the game only through a few
   doors ([Extensions](develop/EXTENSIONS.md#the-doors-into-the-game)).
3. **Every extension can be left out.** Each is a cargo feature; with all of them off, the game
   builds and plays exactly as with all of them on (the `vanilla` suite). The default feature,
   `more`, holds every stable extension: that build is **Un-Banished More**, the program every
   release ships. At run time, `--vanilla` turns them all off for one run. Extensions still in
   development are `unstable-*` features, which no release contains.
4. **No change of results.** A change to how the code is built must leave saves, frames and
   random numbers exactly as they were ([COMPATIBILITY.md](COMPATIBILITY.md)). Anything that
   behaves differently on purpose is marked `Deviation:` and listed in
   [DEVIATIONS.md](DEVIATIONS.md).

## One update, step by step

The game runs 60 updates a second. Each update, in a single-player game:

1. **Input becomes commands.** A click on the map, a button in a window, a key: the
   presentation turns each action that changes the town into a `PlayerCommand` (place a
   building, drag a road, change the speed, set a workplace's workers…).
2. **The session decides when.** `GameplayLoop::execute` asks the session. In single player
   (`Local`) the command is carried out at once, exactly where the original carries it out, so the
   random numbers are drawn in the same order. In multiplayer (`LockstepSession`) it waits for its
   slot.
3. **The simulation carries it out.** `Simulation::execute` changes the town.
4. **The simulation runs.** For the game speed's number of iterations, `Simulation::iterate`
   moves the town on: citizens, jobs, buildings, nature, the calendar, the weather. After them
   come the achievements, the smoothing phase and the event log (`Simulation::frame` runs the
   same steps with no window).
5. **The presentation follows.** Each iteration returns `StepEvents`: the models that changed,
   the sounds to play where, whether the terrain changed shape. The presentation reads the town
   to draw it, plays the sounds and refreshes the windows.

Loading and saving go through the core too: `Simulation::from_save` loads a save with no drawing
data, and `Simulation::write_state` writes what a save keeps. A test plays two real saves both
ways, through the simulation alone and through the whole game. Their digests match at every
check (`apps/game/src/gameplay/sim_parity_tests.rs`).

## Multiplayer and the server

Multiplayer is lockstep. Every computer runs the whole town. Only the players' commands
travel between them. A command given at update `u` is carried out at slot `u + delay` on every computer, so all
towns stay identical without sending the town itself.

The server (`unbanished-server`) holds no game data unless its operator gives it some.

- **Rooms.** A game says who it is first (`Hello`: the protocol, its version, fingerprints of its
  game data and mods). A host opens a room; players join with its code. A player whose version,
  data or mods differ from the host's is refused, and told why.
- **Checks.** Every few seconds each player sends the server a digest of its town. When they
  differ, the server keeps the town most players have (or its own, below), and the players go on
  from a save of it.
- **Joining a running game.** The host brings a new player in at a chosen update; everybody goes
  on from one save of that update.
- **Kept towns.** With a `rooms` folder, the server keeps each room's town; a host who opens the
  room again goes on from it.
- **The server's own game.** With `simulate`, the server runs each room's town itself, as one
  more computer that sends no turns. Its town decides every check.
- **On the desktop.** The game can be the server itself: it starts one on a thread when a host
  opens a room at this computer's own address.

The protocol and the room rules are a library with no sockets (`rooms::Relay`), so the server
program, the tests (an in-memory relay) and the desktop game share them.

## Doors and hooks

Extensions reach the game only through a few doors, each owned by the core or the game app:
player commands, step events, a timing hook, world queries, the camera, the Mods window's tabs,
the strings, and the game loop's extension state. [Extensions](develop/EXTENSIONS.md) lists them
with a worked example of a new extension.

Most doors are **observe hooks**: they read the game, and cannot change a result. The timing
hook (`unbanished_sim::timing`) and the step events are examples. **Rule hooks**
(`unbanished_sim::hooks`) let a mod change the game at a fixed point: after each iteration, a
hook may read the date, the citizens and the stock, draw from the game's random stream and post
events. They run from WebAssembly plugins (`unbanished-hooks`, with wasmi), with no floating
point, in name order, with a fixed budget per call. A save lists them, `Hello` carries their
fingerprint so every player of a room has the same ones. A vanilla run loads none.
[Rule hooks](develop/RULE-HOOKS.md) explains them; [Writing a rule hook](guide/WRITING-RULE-HOOKS.md)
builds one.

## What keeps it honest

| Check | What it proves |
|---|---|
| `tools/compat/compare.sh` | Two builds play fixed scenarios with identical saves and frames |
| The `checks` suite | The core depends on no drawing, window, interface, sound or network crate |
| `sim_parity_tests.rs` | The simulation alone gives the whole game's digests |
| The `vanilla` and `features` suites | The game builds and plays the same with no extensions |
| `replay`, `lockstep`, `network`, `multiplayer` | Recordings replay exactly; players stay identical over a simulated and a real network |
| `server`, `server-simulate` | Resyncs, departures, new players, kept towns and the server's own game |
| `hooks` | A rule hook changes the game the same way every time, players with it stay in step, and a player without it is refused |

[Testing](develop/TESTING.md) shows how to run them.

## Where to start reading

| To understand | Read |
|---|---|
| The running town | `crates/sim/src/simulation.rs`, `crates/sim/src/simulation/step.rs` |
| Player commands | `crates/sim/src/command.rs`, `apps/game/src/gameplay/commands.rs` |
| One frame of the game | `apps/game/src/gameplay/frame.rs` |
| Sessions and lockstep | `crates/session/src/session.rs`, `crates/session/src/lockstep.rs` |
| The game app's multiplayer | `apps/game/src/gameplay/network.rs`, `apps/game/src/gameplay/extensions.rs` |
| The protocol and rooms | `crates/net/src/protocol.rs`, `crates/net/src/rooms.rs` |
| The server's own game | `crates/session/src/watch.rs`, `apps/server/src/watch.rs` |
| Extensions | [develop/EXTENSIONS.md](develop/EXTENSIONS.md) |
