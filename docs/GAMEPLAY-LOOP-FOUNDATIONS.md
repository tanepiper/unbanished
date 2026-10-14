# New-game gameplay loop: current port boundary

This note separates the original `Game/Game` startup from the first interactive gameplay loop now
implemented in the port. The original path is traced from Ghidra 12.1.4 pseudocode for the Steam
x64 executable; this is static evidence, not a live-game observation.

## Original handoff

The New Game dialog passes the town name, seed, terrain size, selected `MapGenerator`,
`TerrainGenerator`, and `ClimateGenerator` into `FUN_1400b9a10`. That function constructs the
`Game/Game` update loop (`FUN_14009d530`) and wraps it in the loading transition. The constructor
stores the town name at `+0x28`, seed at `+0x30`, size at `+0x34`, the selected profile pointers at
`+0x40..+0x50`, and the saved-state pointer at `+0x150`.

`FUN_14009bfd0` initializes gameplay managers. On a fresh game it generates terrain with
`FUN_1400b5700`, applies the selected starting condition through `FUN_1400a87d0`, and configures
climate from `Environment/Environment.rsc` plus the selected climate using `FUN_140099300`.
`FUN_14009d0d0` installs the toolbar, event-notification and autosave dialogs, shows the cursor and
fades into play. The detailed profile and placement evidence is in
[`WORLD-GENERATION.md`](WORLD-GENERATION.md) and [`START-CONDITION-PROFILES.md`](START-CONDITION-PROFILES.md).

## Ported interactive slice

The menu now passes the town name, terrain, climate label, starting-condition label, size and seed
into `GameplayLoop`. The loop generates the terrain, enters Early Spring, and draws an interactive
HUD with the selected game identity, year/month and pause, slower, faster and menu controls. The
camera supports drag rotation, middle-drag panning, wheel zoom and WASD/arrows. Monthly lighting
follows the twelve entries in `Environment/Environment.rsc`; the HUD season names come from
`Dialog/StringTable.rsc:seasons`.

At present, only terrain, size and seed affect world generation. The climate and starting-condition
choices are carried into the loop and shown in the HUD, but their simulation effects and starting
entities are not yet applied.

The port's calendar now follows the traced game clock: 3,600 simulation seconds per year, split
into twelve 300-second months, with speed levels 1x, 2x, 5x and 10x. At 1x that is five real
minutes per month; higher levels scale simulation time by their iteration count. The trace is in
[GAME-LOOP-STEP-03.md](GAME-LOOP-STEP-03.md). Pause and speed controls currently operate this
calendar and lighting only. Monthly lighting now interpolates continuously between environment
records, and its date-based autumn and leaf-loss factors reach the foliage shaders. Climate
temperature and annually biased weather thresholds are now traced in
[GAME-LOOP-STEP-05.md](GAME-LOOP-STEP-05.md); stochastic weather transitions and their effect/audio
objects remain unported.
See [GAME-LOOP-STEP-04.md](GAME-LOOP-STEP-04.md).

The original Status Bar resources and serialized weather state are in
[GAME-LOOP-STEP-06.md](GAME-LOOP-STEP-06.md); its `StatusBarUI` controller, update logic and
`UtilityStatusBar` package route are traced in [GAME-LOOP-STEP-07.md](GAME-LOOP-STEP-07.md). Its
click dispatch and controller attachment are closed statically in [GAME-LOOP-STEP-10.md](GAME-LOOP-STEP-10.md).
Population/status meanings and the fixed-stride stock table are recorded in
[GAME-LOOP-STEP-08.md](GAME-LOOP-STEP-08.md); flag producers, stock update owners,
and concrete container types are traced in [GAME-LOOP-STEP-09.md](GAME-LOOP-STEP-09.md) and remain
partially open for tool transitions and stock accounting. The statusbar item's action-2 dispatch,
dialog lifecycle and `StatusBarUI` attachment are traced in
[GAME-LOOP-STEP-10.md](GAME-LOOP-STEP-10.md); live visibility remains unverified.

The outer update and its follow-up are documented in [GAME-LOOP-STEP-01.md](GAME-LOOP-STEP-01.md)
and [GAME-LOOP-STEP-02.md](GAME-LOOP-STEP-02.md). The trace establishes pause/speed gating,
camera input in the game-loop update, elapsed-time entity scheduling, the three packaged UI dialogs
installed at gameplay startup, the original 1x/2x/5x/10x simulation iteration values, and the
calendar's 3,600-second year split into twelve equal months. The environment update and weather
cadence are in [GAME-LOOP-STEP-04.md](GAME-LOOP-STEP-04.md). Toolbar dispatch is in
[GAMEPLAY-SPEED-AND-TOOLBAR.md](GAMEPLAY-SPEED-AND-TOOLBAR.md). The gameplay HUD's camera-input
routing and recent drag-mode correction are recorded in [CAMERA-INPUT-FINDING.md](CAMERA-INPUT-FINDING.md).

The first concrete scheduled component, natural-resource `GrowthComponent`, and its once-per-active
update smoothing pass are traced in [GAME-LOOP-STEP-11.md](GAME-LOOP-STEP-11.md). Its timed child
placement, eight-neighbor density gate, and map-cell transition are traced in
[GAME-LOOP-STEP-12.md](GAME-LOOP-STEP-12.md). Cell admission and linked-resource transition
dispatch are in [GAME-LOOP-STEP-13.md](GAME-LOOP-STEP-13.md). Resource branch coverage across the
shipped templates is in [GAME-LOOP-STEP-14.md](GAME-LOOP-STEP-14.md); its guarded entity creation
and separate category-counter path is in [GAME-LOOP-STEP-15.md](GAME-LOOP-STEP-15.md). The
animation-completion, `AnimateNaturalResource`, and `RemoveNaturalResource` action routes are in
[GAME-LOOP-STEP-16.md](GAME-LOOP-STEP-16.md). Action triggers, counter meanings, and map-byte
meanings still need executor and save/runtime comparison.

## Remaining work before this is a full game loop

- Apply the selected climate profile's effects and the selected start condition's structures,
  households, inventories, raw materials and natural-resource seeding.
- Add the original toolbar/status UI controllers, build and tool modes, object selection, and
  pause/options dialogs. The current controls are a compact HUD implementation, not the full
  packaged toolbar behavior.
- Port weather and seasonal transition rules from the game loop, then connect month changes to
  production, citizens, events and resource consumption.
- Implement save/load, autosave and event notifications in the active game loop.

Desktop and `wasm32-unknown-unknown` `cargo check -p banished-app` both pass for this slice. No
interactive runtime capture was made in this change.
