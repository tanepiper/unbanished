# Multiplayer: a plan that leaves the game unchanged

This is a plan, not a finding: how Un-Banished could let several players run one town together,
without changing the single-player game in any way. Nothing here is built yet. The evidence it
rests on is in [CROSS-PLATFORM.md](CROSS-PLATFORM.md) (native and browser builds simulate
identically) and in the code paths named below. The design follows the same idea as the
Banished Multiplayer mod for the original game (github.com/BarryBareth/banished-multiplayer):
deterministic lockstep, where every player runs the whole simulation and only the players'
commands travel.

## The rule: single player stays bit-identical

The original game is the reference, and multiplayer is not in it. So:

- In a single-player game, every player action takes effect at exactly the moment it does now,
  through the same code, and draws the same random numbers in the same order. Saves and frames
  stay identical; `tools/compat/compare.sh HEAD <new build>` and `determinism.sh` must pass after
  every step below.
- Everything that behaves differently exists only in a multiplayer game, and every such place
  carries a `Deviation:` comment that says so (for the ledger, [DEVIATIONS.md](DEVIATIONS.md)).
- The main menu gets no new button unless the player asks for it (a setting or a separate entry,
  as the mod manager is), so the vanilla menus look as they do now.

## What there is already

| Lockstep needs | In the port now |
|---|---|
| A fixed simulation step | Updates of 1/60 s from an accumulator (`FRAME_STEP`, `apps/game/src/main.rs`); a game speed is that many iterations per update. Game time is integer microseconds. |
| No hidden inputs | One thread; the port's own `crt` maths; ordered maps; the simulation's own `Random`, kept in the save. |
| The same results on every computer | Native (arm64) and browser builds give identical game states over a whole game year ([CROSS-PLATFORM.md](CROSS-PLATFORM.md)). |
| A state hash | `gameplay::digest`: the CRC-32 of the encoded game state (`--sim-digest`, `?digest=`). |
| Join and resync from a save | Saves load and continue identically (`compare.sh`, step 3). |
| Player actions as data | Most window actions are already `work::professions::Command` values (about 20 kinds: workers, crops, limits, demolish, roads, trade, recipes, upgrades…), queued in `construction.commands` and applied by `Agents::apply_commands`. |

## What is missing

### Actions that change the town directly

| Action | Where | Now |
|---|---|---|
| Place a building | `PlacementTool::place` (`apps/game/src/tools/placement_tool.rs`), called from the click in `gameplay.rs` | Pushes the entity, registers it, zones its cells and adds the site at once, then queues it in `construction.placed` |
| Remove (drag) | `RemovalTool::finish` → `apply` (`removal_tool.rs`) | Marks objects and pushes `construction.destroy` and commands |
| Roads (drag) | `tools.rs`: the drag tool, then `town.apply_commands` at once, paused or not (as the original's RoadTool does inside the input handler) | Commands, applied immediately |
| Window buttons | `GameplayLoop::event` → `window_event`, then `apply_commands` if any were queued | Commands, applied right after the event, paused or not |
| Speed and pause | `GameplayLoop` fields `speed`, `paused` | Local state |

### Random numbers drawn by a local action

These draw from the simulation's `Random` at the moment the player acts. That is 1:1 with the
original, so single player keeps it; in multiplayer every computer must make the same draw at
the same point.

| Draw | Where |
|---|---|
| The building variant when a placement tool starts | `PlacementTool::start` (`rng.rand_below(variants)`), from `tools.rs` and the `tool` script command |
| The Path tool's paths | `PathTool::pick` → `find_paths` (`input.rs`, `path_click`) |
| A random town name in the map window | `MapParams::click` (`input.rs`, the Map menu dialog) |

### Local state that pauses the simulation

`pauses_game` (the pause menu) stops the simulation on one computer. In a shared game it cannot;
the menu would open without stopping the town (a multiplayer-only deviation), and pause becomes a
shared command, as speed does.

## The plan

### 1. One command type, applied where it is now (done)

Done in `apps/game/src/gameplay/commands.rs`: `PlayerCommand` and `GameplayLoop::execute`. Every
player action of the inventory above goes through it: window and Professions commands (`Town`),
`Upgrade`, `StartTool`, `Place` (a `Placement`: template, cells, position, angle, variant and
rules), `Drag` (a copy of the drag tool as the drag ended; the tools now say when to apply instead
of applying), `PathQuery`, `RandomTownName`, `RenameTown`, `SetSpeed` and `SetPaused`, the trade
window's hold included. Each is executed at once where its code ran before. Not commands, because
the game itself causes them and every computer would do the same: AutoSlow's speed change, a
tutorial's setup, and the capture script commands `place` and `remove` (they use `Place` and
`Drag` for the change itself).

Proof of no change: `tools/compat/compare.sh` between the build before and after, on the five
scenarios and their continuations: identical saves and frames. The new `actions` scenario drives
the actions through the interface: worker buttons, a placement with rotation, a road drag,
clearing trees, demolishing by drag, the Path tool, a random town name and a rename, speed, and
pause by key. Upgrading a building and the trade window's hold are not in a scenario yet (Save83
has no upgradeable building in view and no trader docked); their code moved without changes.
Also: `determinism.sh 22 1234` identical; `--sim-digest` on
Save83 identical to the digests made before; the workspace tests pass.

The plan as written:

Make every player action a value of one enum, `PlayerCommand`: the existing
`professions::Command` variants, plus `Place { template, cell, rotation, variant }`,
`Remove { rect, kind }`, `StartPlacement { template }` (the variant draw), `PathQuery { entity }`,
`RenameTown { random }`, `SetSpeed`, `SetPaused`. Move the code that changes the town for each
one into an executor, `execute(&mut town, &mut rng, command)`, without changing what it does.

The UI hands each command to a sink. The single-player sink executes it at once, at the same
point the code runs now (after the event, inside the drag handler, at the click), so the order of
everything, random draws included, is unchanged. Only the place where the code lives moves.

The UI keeps what it needs locally: the placement ghost, the drag rectangle, the overlay. A
command that draws a number the UI shows (the variant, the paths) returns it to the player who
gave it.

Proof: `compare.sh HEAD`, `determinism.sh`, and new compat scenarios that drive each action
through the UI with `--press`, `--click-at` and `--script` (place, remove, a road drag, worker
buttons, trade, speed). Before this step the scenarios must cover each action; they are written
first, against the current build.

### 2. Record and replay (done)

Write each executed command with the update it ran at (`--record FILE`), and replay a recording
from the same save (`--replay FILE`), comparing digests. This is lockstep without a network: if a
replay of a recorded game gives the same digests, the commands carry everything that changes the
town. A bug report could carry a save and a recording.

Done in `apps/game/src/gameplay/recording.rs`. A recording is JSON lines (serde): the start, each
`PlayerCommand` with its update, the updates the pause menu held the game (`MenuPause`, which
stops the simulation in the original and is not a command), and a digest every `--record-every`
updates. `--replay SAVE FILE` loads the save without a window, carries the commands out at their
updates (a tool a command starts becomes the current one, as in the interface), and compares
digests. To make every command replayable:

- the queued commands' flush after a window event or a drag is a command (`ApplyQueued`), so it
  is recorded where it happened;
- `Drag` carries the template, cells and settings, and applies the drag tool `StartTool` made for
  that template; `StartTool` also starts the Path tool;
- `RandomTownName` reads the town names itself (before, the Map window read them when it opened).

Proof: the `actions` scenario recorded and replayed, 27 digests identical (and the recorded run
saves the same game state and frame as one without recording); the 1,024-citizen town at speed
10 for 3,600 updates (36,000 simulation iterations) with placements, clearing, demolishing and a
profession change, 13 digests identical; `tools/compat/replay.sh` records and replays the four
save scenarios. `compare.sh` against step 1's build: identical.

### 3. Lockstep in one process

A multiplayer sink sends each command to be executed at update `N + D` on every computer, where
`N` is the current update and `D` the input delay (6 updates, 100 ms, to start). An update runs
only when every player's commands for it have arrived (a player with no commands sends an empty
turn). Commands of one update run in a fixed order: by player, then by the order given.
Commands run at the start of the update, before the speed loop, whether the game is paused or
not, as window commands do now.

Test it before any network: two or three `GameplayLoop`s in one process, joined by a simulated
network with delay, jitter and loss, compared by digest every update. The Banished Multiplayer
mod tests with several game instances the same way.

### 4. Desync checks and resync

Every 600 updates each player sends its digest. On a mismatch, or when a player joins, the host
writes the game (`GameplayLoop::save_file`), sends it, and everybody loads it and continues from
that update. The host's autosave and saves are the only ones written.

For diagnosis, a ring of the last few thousand random draws with their callers, written out on a
mismatch, finds the first difference (the mod's `ringdiff` idea). Digests per part of the state
(citizens, buildings, stock, wildlife) narrow it down further.

### 5. Network

A `Transport` trait with one message stream per player. WebSocket works both natively and in the
browser, so it is the first transport: a small relay (`apps/relay`, a Rust binary with room
codes) that the host and players connect out to, so nobody forwards ports. A direct native
connection (TCP or QUIC) and WebRTC between browsers can follow.

Messages: `Hello` (engine version and build, a hash of the game data and the mod list, which must
match), `Turn { update, commands }`, `Digest { update, crc }`, `Save { update, bytes }` (join and
resync), and later cursors and pings.

### 6. Interface

A Multiplayer window (host, join with a room code, the relay address), a bar with the players,
and the speed and pause shared. New dialogs are the port's own (as the mod manager is), never
changes to the original's.

### 7. Later: separate towns

The mod's other mode gives each player a town on one map. In the port the town, the map cells and
the wildlife are one `StartingTown`, so this needs an owner on every entity and per-player stock,
jobs and statistics. It is a larger change than everything above and comes only after one shared
town works.

## Risks and open questions

- **More platforms.** x86_64 native, Firefox and Safari have not been compared yet (see
  [CROSS-PLATFORM.md](CROSS-PLATFORM.md)); each needs a digest run before players on it can
  join.
- **New games.** Map generation in the browser has not been compared with native. A shared new
  game can start from a save the host writes, which needs no comparison.
- **Speed.** The slowest computer sets the pace. A large town at speed 10 in the browser may not
  keep up; the game would have to slow everybody down or warn.
- **Same data.** Every player needs the same game data and mods. The `Hello` hash checks it; the
  game data cannot be sent, as each player supplies their own copy.
- **Hidden reads.** Anything in the simulation that reads local state (the camera, an open window,
  the frame rate) would split the games. Step 3's in-process test finds those; none are known.

## Order and size

| Step | Delivers | Vanilla check |
|---|---|---|
| 1 | One command type; UI scenarios for each action | `compare.sh`, `determinism.sh`, the new scenarios |
| 2 | `--record` / `--replay` | Replay digests equal the recorded run |
| 3 | Lockstep in one process | Single-player path untouched |
| 4 | Desync checks and resync | — |
| 5 | WebSocket relay and transport | — |
| 6 | Multiplayer window and bar | Vanilla menus unchanged |
| 7 | Separate towns (later) | — |

Steps 1 and 2 are worth doing even without multiplayer: they give replays, reproducible bug
reports and UI tests of every player action.
