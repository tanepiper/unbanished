<!-- The approved plan (2026-10-20). Each phase is built in its own session from this text; tick a phase in its heading when its gate passes. -->

# Un-Banished as a core and extensions: the core is always Banished, everything else is built around it (multiplayer and a server anyone can run first)

## Context

Multiplayer works (docs/MULTIPLAYER-PLAN.md, steps 1–6: commands, record/replay, lockstep,
desync resync, a WebSocket relay, the Multiplayer window), but it is woven into the game app. One
struct, `GameplayLoop` (apps/game/src/gameplay.rs, ~80 fields), holds the simulation (town,
calendar, `Random`, weather, statistics, events, speed, pause), the interface, rendering, sound,
recording and the multiplayer session (lockstep, outbox, net state). The running town itself
(`StartingTown`, apps/game/src/start/, ~5,800 lines) lives in the app crate, and the simulation
step (`gameplay/simulation.rs::iterate`) reads terrain heights from the *rendering* scene view.
As multiplayer grows (joining running games, rejoining, servers), every change risks the 1:1
single-player game.

The user's direction (decided): think of the app as a **core** (Banished 1.0.7, 1:1, always
works) with **everything else built around it**: multiplayer and any other extension of
the engine; **split the core first**; then a **client/server protocol**
where a server is first a **coordinator** (no game data) and later an optional **simulating
authority**; servers run as **one binary / Docker** and **from inside the desktop game**.
This plan is meant to be confirmed once, then built step by step without redesign.

## The principle: a core, and everything else built around it

The **core** is the original game, complete and 1:1: the simulation *and* its original
interface, rendering and sound. Everything Un-Banished adds that Banished 1.0.7 does not have is
an **extension**, built around the core through a small set of **extension points**, never by
editing core code paths. Multiplayer is one extension; the others already exist:

| Extension (today) | Where it lives now | Extension points it needs |
|---|---|---|
| Multiplayer (session, network, server) | gameplay/{lockstep,network}.rs, crates/net, apps/server | command sink, step hook, save/load, digest, menu entry, overlay |
| Recording and replay | gameplay/recording.rs | command sink, step hook, save/load, digest |
| The probe and test hooks | probe.rs, gameplay/probe.rs, `--probe`, `window.unbanished` | read-only queries, input injection, headless runner |
| Cinematic camera and video filters | view/cinematic.rs, tools/videos.sh | camera override, headless runner |
| Mod sites (Nexus Mods, banishedinfo.com) | crates/{nexus,banishedinfo}, ui/mod_browse.rs, platform/nexus.rs | window/tab registration, platform services |
| Browser shell (start page, manager, installed app, Exit) | index.html, web/, platform/web.rs | platform services, exit hook |
| Browser adaptations (resolution %, frame limit, full screen) | app.rs (wasm32 parts) | platform services (already Deviations) |

Rules:
1. **Extension points are the only doors**, all owned by the core and kept small:
   - *command sink*: every player action is a `Command`; an extension can route it (a session),
     record it, or inject it (the probe);
   - *step hook*: observe each update (`StepEvents`), never change it;
   - *read-only queries*: the `Simulation` and the UI tree, for the probe, overlays and servers;
   - *save/load and digest*;
   - *presentation hooks*: a menu entry, a window or tab, an overlay, a camera override;
   - *platform services*: storage, files, network, exit, full screen;
   - *headless runner*: drive a game without a window (captures, `--probe`, servers).
2. **Every extension can be switched off.** Each is a cargo feature (`multiplayer`, `replay`,
   `probe`, `cinematic`, `mod-sites`, `web-shell`), on by default. With every feature off, the
   game builds and plays exactly as now: a `vanilla` build is checked by `compare.sh` against
   the full build.
3. **An extension never changes the core's results.** Anything an extension makes behave
   differently is a `Deviation:` and is limited to games where the extension is in use (as
   multiplayer is today).
4. **The dependency direction is checked**: the core crates never depend on an extension crate.

## Hooks and modes (added 2026-10-20)

The pattern is close to model–view–controller. The **model** is the core simulation
(`unbanished-sim`): deterministic, changed only by `Command`s. The **controller** is the session
layer, which decides when commands reach the model (at once in single player, in lockstep turns
in multiplayer). The **view** is everything that reads the model: the presentation, the probe,
overlays, a server. Multiplayer changes the game only through that translation layer.

Things that reach into the running game come in two kinds:

- **Observe hooks** run at fixed points of a step (before and after an update, when an event is
  posted, when a building is finished) and can only read. They cannot change any result, so
  they are always safe: the probe, metrics, overlays and recording use them. Built in phase X.
- **Rule hooks** change the game at a fixed point (for example a mod's new resource rule or
  event). They change results, so they follow the discipline of BanishedKit mods: deterministic,
  run in a fixed, declared order, listed in the multiplayer `Hello` (every player must run the
  same set) and recorded in saves. Designed and built in phase G, after the server.

**Modes.** A runtime switch, **vanilla**, loads only the core and the original's own mod
system (BanishedKit `.pkm` mods, which are part of Banished) and turns every extension and rule
hook off. The **full** mode adds the extensions (multiplayer, the probe, mod sites, the browser
shell) and, later, rule hooks. One program serves both; a vanilla run is what `compare.sh` and the
vanilla suite check. The cargo features of phase X remain for builds that leave extensions out
entirely.

**Documentation.** Every extension point, hook and mode gets plain-English documentation with a
worked example: what it is for, what it may and may not do, a small complete example, and how to
test it. These live in docs/develop/ (one page per topic, linked from docs/ARCHITECTURE.md) and
follow the project's style (short sentences, words that translate well).

## Target architecture

```
Extensions     multiplayer (session, protocol, server) · replay · probe · cinematic · mod sites · web shell
               ─────────────── extension points (owned by the core) ───────────────
L1  Presentation   apps/game: the original's renderer, UI, audio, input -> Commands
L0  Core           crates/sim (new, unbanished-sim) on top of core/formats/terrain/path/game
```

Multiplayer as an extension, in its own layers:

```
Server     apps/server (unbanished-server): rooms, lobby, persistence; stage 2 simulates
Protocol   crates/net (unbanished-net): versioned messages + transports (WebSocket, in-memory)
Session    crates/session (new, unbanished-session): Local | Lockstep | Replay drive a Simulation
```

Dependency rule (enforced by a test): a layer depends only on layers below it.
`unbanished-sim` must not depend on wgpu, winit, unbanished-ui, unbanished-gfx, unbanished-audio or
unbanished-net; `unbanished-session` not on the presentation crates. It builds for native and
wasm32 and runs with no GPU.

**L0 Core — `unbanished-sim`.** `Simulation` owns everything deterministic: the town (today's
`StartingTown`), the terrain heights (moved out of `SceneView`), calendar, `Random`,
environment (+ template, months), stock warnings, statistics, event log, achievements state,
update count, speed, pause, town names. API:
`new_game(params, &Archive)`, `load(bytes, &Archive)`, `save(&ViewState) -> bytes`,
`execute(Command) -> Outcome`, `step(dt) -> StepEvents`, `digest()`, read-only accessors.
`StepEvents` carries what presentation needs, in today's order: sounds at positions (the
animation events), models changed, terrain reshaped, events posted. `ViewState` (camera,
ambience, window settings) is what a save stores from presentation; a server passes the last one
it received.

**L1 Presentation — apps/game.** `GameplayLoop` keeps UI, rendering, audio, input, tools'
ghosts and overlays, and holds a `Session`. It turns input into `Command`s, reads the
`Simulation` to draw, and consumes `StepEvents`. Model instances (start/models.rs), status icons
(start/icons.rs) and the `carry_models` cache move here.

**L2 Session — `unbanished-session`.** `Session` trait over a `Simulation`: `Local` (single
player: executes a command at once, exactly where it runs today, so order and random draws are
unchanged), `Lockstep` (today's gameplay/lockstep.rs + the NetState part of network.rs: turns,
delay, digests, resync, leaving), `Replay` (recording.rs). Talks to a `Transport` from L3, never
to the UI.

**L3 Protocol — `unbanished-net` v1.** Versioned and split in two: *lobby* (`Hello{protocol,
engine version, data hash, mod list hash}`, host/join/list rooms, password, players, leave,
ping) and *game* (`Start`, `Turn`, `Digest`, `Save`, `Join{from_update}`, `Rejoin`,
`Leave`). Transports: WebSocket (native thread + browser, as now) and in-memory (tests).
Before 1.0 there is no compatibility promise; v1 replaces the relay protocol in one cut.

**L4 Server — `unbanished-server`** (grows out of apps/server).
- *Stage 1, coordinator (no game data):* rooms and lobby; keeps the latest save and the turn
  log since it; compares digests itself (majority, ties to the host) and asks a matching player
  for a save to resync; join mid-game and rejoin (send save + turns since); rooms persist on
  disk, so a town resumes when players come back; health endpoint; config file; Dockerfile;
  TLS through a proxy (documented, Caddy).
- *Desktop hosting:* the desktop game can run the same server on a thread ("Host on this
  computer" in the Multiplayer window, a Deviation). The browser cannot listen, so it only
  joins.
- *Stage 2, simulating authority (optional):* `unbanished-server --simulate WINDATA` holds a
  headless `Simulation` (L0), joins its own room as a lockstep peer that sends empty turns,
  writes the saves and decides desyncs by its own digest. The operator supplies their own
  Banished data.

## Phases (each one a separate build session, verified before the next)

Every phase keeps single player bit-identical. Gate for each: `tools/test/run.py standard`
(probe baselines unchanged), `tools/compat/compare.sh HEAD <build>` identical, and for phases
touching multiplayer `tools/test/run.py replay lockstep network multiplayer`. Large moves use
tools/refactor (split_impl.py, regroup.py) so code moves without being rewritten.

**R. Rename the code to `unbanished` (first, mechanical, zero behaviour change).** — **Done** (a8c1e73).
- Crates: `unbanished-core` → `unbanished-core`, and the same for terrain, path, game, formats,
  ui, gfx, audio, wma, mods, nexus, banishedinfo, net, and the apps (`unbanished-app` →
  `unbanished-app`, viewer, cli, modmanager, relay). Rust paths follow
  (`unbanished_game::` → `unbanished_game::`). New crates in later phases use the name from the
  start (`unbanished-sim`, `unbanished-session`, `unbanished-server`, extension crates).
- Programs: `unbanished` → `unbanished`, `unbanished-mods` → `unbanished-mods`, `unbanished-cli` →
  `unbanished-cli`, `unbanished-server` → `unbanished-server` (in D).
- Environment variables: `UNBANISHED_*` only, with no fallback to the old names (decided
  2026-10-20: before 1.0 there is no backward-compatible naming); one helper,
  `unbanished_formats::env::var("USER_DIR")`, holds the prefix. `tools/compat` sets both names,
  because it also runs builds from before the rename.
- **Not renamed, on purpose:** what holds players' data: the settings folder `banished-wasm`,
  the browser's IndexedDB database `banished-wasm` and localStorage key
  `banished-wasm.settings`, the save folder (`Documents/Banished`, the original's), the original
  game's resource names, and the menu's "Version 1.0.7 - Build 170910". Renaming those would
  lose settings, kept game files and saves.
- Everything that names the old ones: tools/*.sh, tools/test/run.py, .github/workflows,
  site/scripts, Trunk.toml, the READMEs, docs, skills, AGENTS.md (its rule "keep the working
  name, don't rename code identifiers" becomes "code is named `unbanished`; player-data names
  stay `banished-wasm`").
- Done with one script in tools/refactor (crate and path renames, then `cargo fmt` on touched
  files), in a single commit. Gate: the full gate above plus `tools/test/run.py long`; the
  probe baselines and compare.sh unchanged (compare.sh takes the old binary path as BASE).

**A. Core split (zero behaviour change).**
1. **Done** (a1779d3). Terrain heights out of the scene view: a core `Terrain` (heights, width) owned by the game
   state; `SceneView` reads it and rebuilds its mesh on "reshaped". Touches
   gameplay/simulation.rs (`apply_commands`, `iterate`), view/scene.rs, tools that build a
   `HeightGrid`.
2. **Done** (7101c81; `view/town`, `start/save/view_state.rs`). Presentation out of the town: start/models.rs, start/icons.rs and `carry_models` to
   presentation; camera and ambience in saves (start/save/new_game.rs) come in as `ViewState`.
3. Create `crates/sim`: move start/* (minus 2), saves/saved.rs, the core part of
   gameplay/commands.rs (`PlayerCommand` → `Command`, `execute`), gameplay/simulation.rs, the
   calendar/environment fields, and the world queries of gameplay/probe.rs. `GameplayLoop`
   holds `sim: Simulation`.
   - 3a **done** (1a19346): `start::map::Map` holds the generated or saved map with no mesh
     or textures; `view::world::World` builds on it.
   - 3b **done**: `crates/sim` (`unbanished-sim`) holds the town (`town`, used by the app as
     `start`), `saved` and `mods`. Its test helpers are behind the `test-support` feature,
     which only the app's tests turn on. It depends on no GPU, window, UI, sound or network
     crate.
   - 3c **done**: `unbanished_sim::simulation::Simulation` holds the town, heights, calendar,
     random stream, environment, stock warnings, statistics, event log and update count;
     `Simulation::iterate` runs one iteration and returns `StepEvents` (models changed, 3D
     sounds, terrain reshaped), which the app plays and draws. `GameplayLoop.sim` holds it.
     `unbanished_sim::timing` lets the app's profiler time the step's parts. Left for A4:
     `PlayerCommand` → `Command` and its `execute`, and the probe's world queries.
4. `StepEvents` and read-only accessors replace direct field reads from presentation.
   - **Done**: the tools' core (`unbanished_sim::tools`: placement, removal and roads, paths,
     `Entity`, and the overlay and drag data they report) and the player commands
     (`unbanished_sim::command`: `PlayerCommand`, `Outcome`, `Simulation::execute`) are in the
     sim. The app's `execute` routes a command to lockstep or the recording, handles the speed,
     the pause and the town's new name, and passes the rest on. `StepEvents` came in A3c.
     Read-only accessors wait for phase X, where the extension points define what they read.
5. New check in run.py `checks`: `cargo tree -p unbanished-sim` has no wgpu/winit/ui/gfx/audio/net;
   a test that loads a save with `unbanished-sim` alone (no GPU) and matches `--sim-digest`.
   - **Done**: run.py `checks` fails when `cargo tree -p unbanished-sim` (every target) lists
     wgpu, winit, unbanished-ui, unbanished-gfx, unbanished-audio or unbanished-net.
     `crates/sim/tests/headless.rs` loads Save83 with the sim alone (`Simulation::restore`,
     `Map::from_save`, `StartingTown::from_save`), plays ten seconds, writes the state and
     reloads it; two runs give the same bytes. It does not match `--sim-digest` yet: see 6.
6. The frame's simulation update in the sim, so a headless run matches `--sim-digest`: the
   speed loop, the smoothing after it, goals and achievements, and the life and site events.
   The events carry translated strings into the event log (and so into the save), so the sim
   takes a string lookup from the app. Loading moves too (`Simulation::from_save`,
   `Simulation::new_game`: the map, the town and the environment). Then the headless test
   compares with `--sim-digest` on Save83 and Driscola.
   - **Done** (for loaded games): `Simulation::from_save` loads a save with no drawing data;
     `Simulation::frame` runs a frame (the speed's iterations, the achievements, the smoothing,
     the life and site events, AutoSlow); `Simulation::saved_state` and `write_state` write
     what a save keeps. The game's text comes through `unbanished_sim::strings::Strings`, which
     the app answers from its string tables (`ui::strings::UiStrings`). The achievements moved
     to `unbanished_sim::achievements`; the app only keeps what was awarded in its settings.
     `gameplay/sim_parity_tests.rs` plays Save83 and Goldfoothill both ways for 300 updates
     and the digests match `--sim-digest` at every check (Driscola needs its missing mods
     answered, so it is not in the test). The app still runs the frame's steps itself, with
     the drawing and a scenario's goal between them; the parity test keeps the two in step.
     Left for later: `Simulation::new_game` (a new game's ambience regions are counted by the
     app's audio code) and a scenario's goal.

**X. Extension points and features (after A, before B).** Define the extension points in the
core as small traits/hooks (command sink, step hook, presentation hooks: menu entry, window or
tab, overlay, camera override; platform services; headless runner) and move the existing
extensions behind them, each under its cargo feature: `probe` (probe.rs, gameplay/probe.rs,
`--probe`, web/probe.js), `cinematic` (view/cinematic.rs), `mod-sites` (nexus, banishedinfo,
mod_browse), `web-shell` (start page manager, exit), `replay`, `multiplayer` (today's code as is,
reorganised in B). The Multiplayer button becomes a registered menu entry. Observe hooks (before and after an
update, on an event posted, on a building finished) with a test that a hook which reads
everything leaves the game unchanged (compare.sh with a hook that reads every field). The
**vanilla** switch (a launch option and a setting; the browser start page offers it too),
with a suite that runs compare.sh between a vanilla run and the vanilla build. Docs:
docs/develop/EXTENSIONS.md (the extension points, with an example extension),
docs/develop/HOOKS.md (observe hooks, with an example that counts births), and a section on
the two modes in the user guide. New check in run.py:
`cargo build --no-default-features` for native and wasm32, and a `vanilla` suite that runs
`compare.sh` between the vanilla build and the full build (identical saves and frames). One
session per extension moved, smallest first (cinematic, probe, mod sites, web shell, replay).
   - **Started**: `cinematic`, `probe` and `mod-sites` are cargo features of the game, on by
     default. The doors that exist are listed in docs/develop/EXTENSIONS.md (commands, step
     events, timing, world queries, the camera, the Mods window's tabs, strings), with a worked
     example. run.py has a `features` suite (standard: no extensions, native and wasm, 0
     warnings) and a `vanilla` suite (long: compare.sh between the build without extensions and
     the full build; identical). `replay` and `multiplayer` become features in B, when their
     code leaves the game loop. The browser's start page is the web page itself, not a
     feature. Still to do in X: the vanilla runtime switch, and the Multiplayer button as
     a registered menu entry.

**B. Session layer.** Create `crates/session`: move gameplay/lockstep.rs, the session half of
gameplay/network.rs and gameplay/recording.rs; `Local`, `Lockstep`, `Replay` behind `Session`.
`--lockstep-test`, `--net-test`, `--replay` run on it. Gate adds lockstep/network/replay suites.
   - **B1 done**: `crates/session` (`unbanished-session`) holds what needs no game loop: the
     lockstep turns, messages and `Lockstep` (with its tests), the `Link` trait and the relay,
     shared and simulated links, `Digest`, and a recording's `Entry` and `Recorder`. The game
     loop's glue (joining, slots, resyncs, `NetState` with its players line, `NetPeer`, the
     test harnesses) stays in `apps/game/src/gameplay/` and re-exports the moved types.
   - **B2 done**: `unbanished_session::session::Session` says when a game carries out its
     players' commands: `give` (now, or held for its slot), `slot` (the commands before an
     update, or wait), `take_outbox`. `Local` carries everything out at once;
     `LockstepSession` owns the `Lockstep`, the outbox and the last slot, and `resynced` picks
     up after a resync. The game loop holds an `Option<LockstepSession>` and asks it; a server
     can drive a `Simulation` with the same session. Replays stay a driver of their own
     (`recording::replay`), since the recording also holds menu pauses and digests.
   - **B3 done**: `replay` and `multiplayer` are features (`multiplayer` turns on `replay`).
     The game loop's multiplayer and recording fields are two structs behind one door
     (`gameplay/extensions.rs`: begin an update, route and record a command, whether it is
     online or in lockstep), with empty stand-ins without the features. The Multiplayer window
     and its menu button have a stand-in (`ui/multiplayer_off.rs`), so the menu's code is the
     same in both builds. `unbanished-session`'s relay link is its `relay` feature. Every
     combination builds with no warnings.
   - **Vanilla runs done**: `--vanilla` (the browser's `?vanilla`) sets `platform::mode::vanilla()`,
     and each extension's door checks it: no Multiplayer button, no Browse tabs, no probe, no
     recording, no cinematic camera. With that, phase X is done.

**C. Protocol v1.** Versioned `Hello` handshake (rejects mismatched engine, data or mods with a
readable reason), lobby/game split, `Join{from_update}`, `Rejoin`, `Ping`. In-memory transport.
Unit tests: round trips, bad versions, truncated messages.
   - **Done** (the lobby half): `unbanished-net` speaks protocol 1. A game says `Hello`
     (protocol, engine version, fingerprints of its game data and mods from
     `Archive::fingerprints`); a relay of another version refuses it with the reason, and a
     room refuses a player unlike its host and says what differs (version, data or mods). The
     lobby has `ListRooms`, a room password, `Leave` and `Ping`. The relay's rules are a
     library (`rooms::Relay`, no sockets), used by the relay program and by an in-memory
     transport (`memory::MemoryRelay`); tests cover round trips, cut-off messages, a wrong
     version, a wrong password, unlike players and the lobby. `RelayLink::new` sends the game's
     `Hello` first. The game half (`Join{from_update}`, `Rejoin`) comes with the server in D,
     which is the first thing to answer them.

**D. Server stage 1 + desktop hosting.** apps/server (relay folded in): coordinator, digest
arbitration, join/rejoin, persistence, config, Dockerfile, docs; the Multiplayer window gains
"Host on this computer" and rejoin. New suite `tools/compat/server.sh` (long tier): a server
and three headless players — join, play, one leaves and rejoins mid-game, a scrambled player is
resynced, the server restarts and the town resumes; all digests agree.
   - **D1 done**: the relay is `apps/server` (`unbanished-server`): a settings file
     (`--config`), `GET /health`, `max_rooms`, a Dockerfile (not built here: it downloads the
     base images) with a `.dockerignore` that keeps game data out, and the README covers TLS
     through Caddy. Modules: `config`, `health`, `serve`. The compat scripts build the server
     they start.
   - **D2 done**: the server checks the games (protocol 2). Each player sends it its digest
     (`ToRelay::Digest`); `unbanished_net::checks::Checks` decides once every player in the
     room has sent one: they agree, or the game most players have is kept (the host's on a
     tie) and the lowest-numbered player with it is asked for its save (`SaveWanted`), which
     it sends at its next update boundary as the host did. Digests carry the resync epoch, so
     one from before a resync is never compared with one after. A game without such a server
     (the one-process lockstep test) still has the host check (`Link::send_digest` is false).
   - **D3a done**: joining a running game, on by default. The host hears of a newcomer at
     update J and announces it to everyone (`Grow { player, from: S }`, S = J + 2·delay + 2:
     every turn for slot S on is sent once the newcomer is in the room, and nobody has run slot
     S yet). At update S − 1 the host sends its save to the others (a resync) and a `Start` at
     that update to the newcomer, with the speed, the pause and the drag tools a save does not
     hold. Everyone goes on from that one save, so every town has its templates in the same
     order; after any reload the drag tools start again by template and the Path tools start
     afresh. The newcomer keeps what arrived before its `Start`. A player who left joins again
     the same way, as a new player number. `network.sh` gains a `late` run.
   - **D3b done**: rooms kept on disk, only when the server's settings name a folder
     (`rooms = DIR`). Protocol 3: `Hosted` says whether the server keeps rooms and whether
     this code has a kept town; the host sends `Keep` (its save) at the start, at every resync
     or join, and every 3600 updates; `Fetch` answers `Kept`. The room rules take a `Store`
     (`unbanished_net::store`: a folder store writes `CODE.sav` and `CODE.update` beside and
     renames them over, a memory store for tests); only codes of letters and digits, at most
     16, become file names. The Multiplayer window, hosting a room with a kept town, goes on
     from it at **Start**. A server restart keeps the towns: the end-to-end check is in
     `server.sh` (D4).
   - **D4 done**: the server is a library too (`unbanished_server::spawn`). A desktop host who
     opens a room at `ws://127.0.0.1:PORT` (or `localhost`) with no server there starts one on a
     thread, listening on every network, and the window shows the address the others enter
     (`platform::host_here`). `tools/compat/server.sh` (run.py's `server` suite, long tier):
     three players, a scrambled one resynced, one who leaves, one who joins; the server keeps
     the town, restarts on the same folder, and the host goes on from the kept town with
     another player; every digest agrees. With that, phase D is done. The Docker image was
     built and checked (143 MB): it runs as `nobody`, answers its health check (`healthy`),
     keeps rooms in a `/rooms` volume, and a host resumed a kept town after a restart.

**E. Server stage 2.** `--simulate WINDATA`: the server as a headless peer and authority.
`server.sh --simulate`: same scenario, plus a game that advances with one player and the
server's digests are the reference.
   - **Done**: `simulate = DIR` (or `--simulate DIR`). The server loads the operator's game data
     once and, for each room whose host has the same data and no mods, runs the game on a
     thread (`unbanished_session::watch::Watcher`, `apps/server/src/watch.rs`). It is not a
     lockstep player (it sends no turns, so it never holds anyone up): it hears every message
     a player sends the room and repeats the game app's frame update by update. Its digests
     join each check as player 255 and always win; when the players differ from it, it sends
     its own save (and reloads from it, as the players do). Event text comes from the archive
     (`unbanished_sim::strings::ArchiveStrings`, checked against the interface's). A multiplayer
     game now counts achievements from none won for every player (a Deviation), or players with
     different achievements would differ. `server.sh --simulate` (run.py `server-simulate`):
     two players agree with the server at every check with no resync; a scrambled player is
     resynced from the server's save; the departure, the new player and the restart pass too.

**G. Rule hooks and a mod hook API (after E).** Rule hooks at fixed points of the step,
deterministic and ordered, declared by the mod that adds them; listed in `Hello` and in saves;
refused in a vanilla run. A small, versioned API for mods to use them (the language and the
sandbox, for example WebAssembly plugins, are decided at the start of G). Proof: compare.sh is
unchanged with no rule hooks; a test mod with one rule hook replays identically and two players
with it stay in step; a player without it is refused at `Hello` with a readable reason. Docs:
docs/develop/RULE-HOOKS.md and a mod author's guide, each with a complete example mod.
   - **Done**: WebAssembly plugins run with wasmi (no floats, the `deterministic` feature,
     2,000,000 fuel per call, 16 MiB of memory), from the `Hooks` folder in name order. The
     core's door is `unbanished_sim::hooks` (`RuleHooks`, `HookContext`); `Simulation::iterate`
     calls it at the end of each iteration. API version 1: the date, the citizens, the stock,
     `random_below` from the game's own stream (moved into the plugin's store for the call) and
     `post_event`. Protocol version 4: `Hello.hooks` is the plugins' fingerprint, and a player
     whose hooks differ is refused with "this game's rule hooks differ from the host's"; the
     server's own game takes no room with hooks. Saves list each hook as `Hooks/NAME.wasm` with
     its CRC. `--vanilla` loads none. `tools/compat/hooks.sh` (run.py `hooks`): a test plugin
     changes a save's digests, gives the same ones twice and none under `--vanilla`; two players
     with it stay identical through a server; a player without it is refused. The example mod
     is `examples/rule-hooks/harvest-festival`; the docs are
     [RULE-HOOKS.md](develop/RULE-HOOKS.md) and [WRITING-RULE-HOOKS.md](guide/WRITING-RULE-HOOKS.md).

**F. Docs.** docs/ARCHITECTURE.md (the core, the extension points, the features, the
dependency rule, and a short "how to add an extension" guide with one worked example);
AGENTS.md: a rule that new behaviour not in the original is an extension behind a feature;
MULTIPLAYER-PLAN.md updated; guide: running a server (binary, Docker, desktop, TLS);
CHANGELOG; AGENTS.md map.
   - **Done**: [ARCHITECTURE.md](ARCHITECTURE.md) explains the core and its extensions in plain
     English: the layers and what each crate holds, the four rules, one update step by step,
     multiplayer and the server, doors and hooks, the checks that keep it honest, and where to
     start reading. It and [develop/EXTENSIONS.md](develop/EXTENSIONS.md) are on the website.
     The crate list (`develop/CRATES.md`, with the real dependencies), the README, the
     developer index, AGENTS.md's map and MULTIPLAYER-PLAN.md are up to date. The server's
     README covers running it as a program, in Docker, from the desktop game and behind TLS.
     Left in the plan: phase G (rule hooks and the mod API).

## Critical files

- apps/game/src/gameplay.rs, gameplay/{simulation,commands,lockstep,network,recording,probe}.rs
- apps/game/src/start.rs and start/* (models.rs, icons.rs, save/new_game.rs)
- apps/game/src/view/scene.rs (terrain heights), apps/game/src/saves/saved.rs
- crates/net/src/{lib,native,web}.rs, apps/server/src/main.rs
- tools/test/run.py (dependency check, new suites), tools/compat/*.sh

## Verification

- Per phase: the gate above; any probe-baseline or compare.sh difference is root-caused and
  fixed, not recorded.
- After A: `unbanished-sim` builds alone for native and wasm32 with no GPU crates; the headless
  load matches `--sim-digest` on Save83 and Driscola.
- After X: `--no-default-features` builds for native and wasm32 with 0 warnings; the vanilla
  build plays every compat scenario identically to the full build; each extension's own tests
  still pass with its feature on.
- After D/E: `server.sh` (and `--simulate`) pass; a browser player joins a server through
  wss:// via a local Caddy; a desktop-hosted game is joined by a second desktop and a browser.
- The infographic/perf numbers are re-measured after A and B (no regression expected).

## How it will be built

Each phase (R, A1…A5, X (one per extension), B, C, D, E, G, F) is one session with a written brief taken from this plan,
run one after another (R first, then A; within A in order). Thinking happens here; the
build sessions follow the plan and stop to ask only if a gate fails for a reason the plan did
not foresee.
