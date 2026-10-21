# Multiplayer: design and progress

How Un-Banished lets several players run one town together, without changing the single-player
game in any way. Steps 1 to 6 below are built and tested: player commands, recording and
replay, lockstep, desync checks and resync, the WebSocket relay, and the Multiplayer window with
the players line. Step 7 (a town for each player) is not started. Each step lists how it was
proved. For players, [guide/MULTIPLAYER.md](guide/MULTIPLAYER.md) explains how to play.

**Since then** the work went on as phases B to E of [ARCHITECTURE-PLAN.md](ARCHITECTURE-PLAN.md),
and [ARCHITECTURE.md](ARCHITECTURE.md) describes the result: the session code is its own crate
(`unbanished-session`), the relay became `unbanished-server`, the protocol is versioned (a
`Hello` that refuses a game of another version, data or mods), the server checks the players'
towns itself, players can join a running game, rooms can keep their towns, the desktop game can
be the server, and the server can run each room's game with its operator's own game data. The
steps below describe the first version; where they name the relay or the host's checks, the
server now does that work.

The evidence it rests on is in [CROSS-PLATFORM.md](CROSS-PLATFORM.md) (native and browser builds
simulate identically) and in the code paths named below. The design follows the same idea as
the Banished Multiplayer mod for the original game (github.com/BarryBareth/banished-multiplayer):
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
| Place a building | `PlacementTool::place` (`crates/sim/src/tools/placement_tool.rs`), called from the click in `gameplay.rs` | Pushes the entity, registers it, zones its cells and adds the site at once, then queues it in `construction.placed` |
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

Done in `crates/sim/src/command.rs` (`PlayerCommand`, `Simulation::execute`) and `apps/game/src/gameplay/commands.rs` (`GameplayLoop::execute`, which routes them). Every
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

### 3. Lockstep in one process (done)

A multiplayer sink sends each command to be executed at update `N + D` on every computer, where
`N` is the current update and `D` the input delay (6 updates, 100 ms, to start). An update runs
only when every player's commands for it have arrived (a player with no commands sends an empty
turn). Commands of one update run in a fixed order: by player, then by the order given.
Commands run at the start of the update, before the speed loop, whether the game is paused or
not, as window commands do now.

Test it before any network: two or three `GameplayLoop`s in one process, joined by a simulated
network with delay, jitter and loss, compared by digest every update. The Banished Multiplayer
mod tests with several game instances the same way.

Done in `apps/game/src/gameplay/lockstep.rs`. `Lockstep` keeps the turns: a command given while
`c` updates have begun goes to slot `c + delay`; beginning an update seals the local player's turn
for the slot `delay` ahead and puts it in the outbox; a slot is carried out, and the next update
begins, only when every player's turn for it is there (`GameplayLoop::lockstep_slot`, called by
`update`, which otherwise waits). Commands run by player, then in the order given. Another
player's Path tool is kept for them; drag tools are kept by template for everybody. In a lockstep
game the pause menu does not hold the game (a deviation; a replay still honours the recording's
pauses).

`unbanished --lockstep-test SAVE RECORDING [PLAYERS] [DELAY] [JITTER] [STALL] [SEED]` loads the save
once per player in one process, splits the recording's commands between them (whoever starts a
tool gives its commands; the next tool goes to the next player), and joins them by a simulated
network: a turn arrives 1 to `1 + JITTER` ticks after it is sent. Each computer is too busy to
update in STALL % of ticks. Digests every 60 updates must agree. `tools/compat/lockstep.sh`
records the `actions` scenario and runs six settings.

One thing was shared between towns in one process: the animation manager's instances
(`behaviour::animation::shared`), one per thread as in the original's one per process, whose
instance numbers are saved. The test gives each town its own table (`shared::swap`); on separate
computers each has its own anyway.

Proof: one player with no delay matches the recording's 27 digests (lockstep then carries the
commands out where a replay does); 2 to 4 players with delays 0 to 10, jitter up to 30 ticks and
computers busy up to 60% of the time agree on all digests, at most `delay` updates apart.
`compare.sh` against step 2's build: identical.

### 4. Desync checks and resync (done)

Every 600 updates each player sends its digest. On a mismatch, or when a player joins, the host
writes the game (`GameplayLoop::save_file`), sends it, and everybody loads it and continues from
that update. The host's autosave and saves are the only ones written.

For diagnosis, a ring of the last few thousand random draws with their callers, written out on a
mismatch, finds the first difference (the mod's `ringdiff` idea). Digests per part of the state
(citizens, buildings, stock, wildlife) narrow it down further.

Done in `apps/game/src/gameplay/lockstep.rs`. Every `check` updates (600 by default) each player
sends the host a `Message::Digest` of its game, made at an update boundary. When the host holds
every player's digest of an update and they differ, it writes its game at its next boundary
(`save_file`) and sends `Message::Save`; everybody, the host included, loads it and goes on from
that update (`GameplayLoop::reloaded`). Loading on every computer, not only on the others, means
nothing that lives only in memory can keep the games apart.

The lockstep goes on across the reload: a computer that was ahead puts the turns it already
carried out after the save back (`Lockstep::rewind`, from a history of the last 4,096 slots); one
that was behind seals the local turns it skipped (`seal_through`), so nobody waits for them. The
tools, the speed and the pause carry over (they are not in saves and are the same everywhere).
Each resync starts a new epoch; a digest from an older epoch is ignored, so a digest made before
the resync cannot call for another.

`--lockstep-test` takes the check interval and a corruption, `PLAYER:UPDATE`, which scrambles that
player's game `Random` there. Proof (`tools/compat/lockstep.sh`, eight settings): without a
corruption no desync is found and no resync happens; player 1 scrambled at update 400 is found at
the check of 480 and resynced at 485, and the three players agree on every digest after it;
player 3 of four scrambled at 1,000 is found at 1,020 and resynced at 1,026. With the checks off,
the same corruption leaves the players disagreeing from update 420, so the comparison is real.

Joining a running game uses the same save; it needs the players to agree on the slot from which
the new player's turns are wanted, and is left for the network step.

### 5. Network (done)

A `Transport` trait with one message stream per player. WebSocket works both natively and in the
browser, so it is the first transport: a small relay (`apps/server`, a Rust binary with room
codes) that the host and players connect out to, so nobody forwards ports. A direct native
connection (TCP or QUIC) and WebRTC between browsers can follow.

Messages: `Hello` (engine version and build, a hash of the game data and the mod list, which must
match), `Turn { update, commands }`, `Digest { update, crc }`, `Save { update, bytes }` (join and
resync), and later cursors and pings.

Done. The relay is `apps/server` (`unbanished-server [ADDRESS]`, default `0.0.0.0:7780`): rooms with
six-letter codes, players numbered as they come (the host is 0), news of who comes and goes, and
data passed on to one player or to all the others without being read. The protocol and a
connection to it are `crates/net` (`unbanished-net`): one postcard-encoded value per WebSocket binary
message; natively a background thread with tungstenite, in the browser the page's `WebSocket`.

In the game, `gameplay/network.rs` holds a `Link` (the relay through `RelayLink`, or a simulated
network for tests) and `NetPeer`, which drives one player's lockstep game over it: it receives
turns, digests and saves, seals and sends its turns, and as the host checks digests and resyncs.
A game starts with the host's `Message::Start`: its save, the players, the delay and the check
interval. Every player, the host included, loads that save, so a joining player needs no file. The
in-process test (`--lockstep-test`) now runs on the same `NetPeer` code.

Proof:

- `tools/compat/network.sh` starts a relay and runs three game processes as the players of a room
  (`unbanished --net-test`): the two that join get the save from the host; all agree on 26 digests
  (1,567 updates in about 16 s). With player 1 scrambled at update 400, the host finds it at 480,
  resyncs at 481, and the three agree on the 18 digests after.
- A browser tab joined the same kind of room as the third player
  (`?net=ws://…&room=CODE&updates=N` on a local page): it received the host's 7.9 MB save over the
  WebSocket and agreed with the native host and the native player on all 27 digests, through every
  action of the recording. Native and browser players can share a game.

Open: a public relay needs TLS (`wss://`) for the published `https://` site, by a reverse proxy in
front of it; a player who leaves is not handled yet (the others wait); joining a game already
running.

### 6. Interface (done)

A Multiplayer window (host, join with a room code, the relay address), a bar with the players,
and the speed and pause shared.

Done. The main menu has a Multiplayer button under Load (a copy of the Load button, marked as a
Deviation; the player chose an always-visible button over a hidden option). The window
(`ui/multiplayer.rs`) is the original's Map dialog with new labels: the room code in the town
name's edit, the relay's address in the seed's (saved in the settings as `MultiplayerRelay`),
the players and the status in two text rows. OK reads Host while the code is empty and Join with
a code; the host's becomes Start once a player is in, and Start opens the Load dialog for the save
to play. Every player, the host included, then loads that save.

The session moved from `NetPeer` into the game (`NetState` in `GameplayLoop`), so the real game
loop runs it: each update reads the messages, carries out its slot, sends the local turn and,
as the host, checks digests. A resync builds the reloaded game and fades to it through the
loading screen. In a multiplayer game every player command waits for its slot (`execute`
returns `Outcome::Queued`), including the speed and the pause, which are therefore shared; the
outcome a player needs (the tool started, a drag's sound, a random town name) is applied when
the slot runs. A placement plays its sound when the click is accepted. The game is not
autosaved. A players line at the foot of the screen (the original's version footer without its
logo) says which player this is, or that the game waits for the others.

Proof: `tools/compat/multiplayer.sh` runs a relay and two headless games kept to the clock. The
host hosts room TEST, the guest types the code and joins, the host presses Start and picks its
save. The guest changes the speed during the game. The host's checks every 60 updates all
agreed (24 checks in 45 s). `tools/compat/lockstep.sh` and `network.sh` still pass on the new
code, and the single-player game is unchanged (`compare.sh`).

**Waiting.** While the game waits for another player's turn, only the town waits: the camera,
the windows and the players line go on. Such a held frame skips everything that changes the
game (the update count, the iterations, the goal and achievements, and the smoothing pass that
draws on the game's Random even at speed zero), because the number of held frames differs from
one computer to the next. A command given while waiting goes into the next turn not yet sent
(before, it went into a turn already sent and was lost). AutoSlow is always on in a multiplayer
game (a Deviation), so the speed does not depend on each player's own setting. After half a
second of waiting the line names the players waited for.

**Leaving.** The relay passes on everything a player sent before the news that they left. Each
computer gets its messages in order. So every computer holds the same turns from that player.
Each takes them out of the game from the slot after their last turn, with no message
of its own (`Lockstep::leave`). The host stops waiting for their digests. The line says who has
left. If the host leaves, the others play on without checks. Players who joined the lobby and
left before the start are named in `Message::Start` (`absent`).

Proof: `tools/compat/multiplayer.sh` has a second scenario, `leave`: the host and two guests, of
whom one quits 30 s into the run. The host found the departure and the two who stayed agreed on
all 19 checks after it. A manual run paused the guest process (SIGSTOP): the host logged that
update 486 waited for player 2, and a camera turn held during the wait turned the view. Unit
tests cover leaving (the slot, readiness, the host's checks) and a command given while waiting.

**False desyncs.** The digest hashed the whole save. A save also holds each player's own view:
the camera, and the settings each building's window remembers (`UIComponent`). So a
player who zoomed, or opened and closed a building's window, looked out of step, and the host
resynced everybody, over and over while the camera moved. Record and replay in single player
showed it: a run with only a zoom, and one with only a window opened and closed, each replayed
to a different digest; `savegame_fields_diff` found the window settings the only other
difference. The digest now leaves the camera and the window settings out (the saves are
unchanged). Both runs replay identically. A two-player run with camera moves, window clicks
and a house placed by each player then agreed on all 32 checks. This is the likely cause of the
one unexplained desync seen earlier.

**The browser.** A browser player joined a desktop host's room through the Multiplayer window
on a local page and agreed with the host on 46 checks; when the host left, the browser played on
and said so. Cancel in the browser's window did not close its connection, so its room stayed
open until the page closed: the browser's connection now closes when it is dropped.

Open: a player cannot rejoin a running game. The room code cannot be copied from the window.
Player guide: [guide/MULTIPLAYER.md](guide/MULTIPLAYER.md).

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
- **Same data.** Every player needs the same game data and mods. The `Hello` fingerprints check
  it (done: a game unlike the host is refused, with the reason); the game data cannot be sent,
  as each player supplies their own copy.
- **Achievements.** A player's achievements already won used to change their town's digest, so
  players with different achievements differed at every check. Fixed: in a multiplayer game
  every player counts achievements from none won.
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
