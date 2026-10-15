# Game loop reverse-engineering and port checklist

Updated 2026-10-15. This checklist tracks the gap between the new-game map view and a complete,
interactive Banished game loop. The linked step documents hold the supporting Ghidra/resource
evidence; static traces are not claims of live-game behavior.

## Completed traces and current implementation

- [x] Trace the New Game handoff: town name, seed, terrain size, selected map/terrain/climate
  profiles, fresh-game setup, and transition into gameplay. See
  [GAMEPLAY-LOOP-FOUNDATIONS.md](GAMEPLAY-LOOP-FOUNDATIONS.md),
  [WORLD-GENERATION.md](WORLD-GENERATION.md), and
  [START-CONDITION-PROFILES.md](START-CONDITION-PROFILES.md).
- [x] Implement the generated map, packaged toolbar/Status Bar, date and pause/speed controls,
  camera navigation and monthly lighting. Selected climate and starting profiles drive their
  currently ported environment, structures, inventory, households and natural resources.
- [x] Trace the outer loop, calendar, climate temperature/weather thresholds, and packaged gameplay
  dialogs. See [GAME-LOOP-STEP-01.md](GAME-LOOP-STEP-01.md) through
  [GAME-LOOP-STEP-10.md](GAME-LOOP-STEP-10.md).
- [x] Trace the first scheduled natural-resource component: growth smoothing, timed child placement,
  neighbor-density gate, map-cell admission, and shipped resource-description branches. See
  [GAME-LOOP-STEP-11.md](GAME-LOOP-STEP-11.md) through
  [GAME-LOOP-STEP-14.md](GAME-LOOP-STEP-14.md).
- [x] Recover the five scheduler quotas and verify growth, initialization and bucket visits against
  the original functions under emulation. See [GAME-LOOP-STEP-18.md](GAME-LOOP-STEP-18.md).
- [x] Recover original microsecond time units and verify loop order/calendar, animation sampling,
  playback clocks, completion and event intervals under emulation. See
  [GAME-LOOP-STEP-19.md](GAME-LOOP-STEP-19.md).
- [x] Trace the guarded natural-resource output and distinguish its category counter from the
  Status Bar stock count. See [GAME-LOOP-STEP-15.md](GAME-LOOP-STEP-15.md).
- [x] Trace `AnimateNaturalResource` and `RemoveNaturalResource`, animation completion, their action
  sequence dispatcher, CharacterComponent update site, task-transition call site, and action
  construction path. See [GAME-LOOP-STEP-16.md](GAME-LOOP-STEP-16.md) and
  [GAME-LOOP-STEP-17.md](GAME-LOOP-STEP-17.md).
- [x] Static camera input audit: map drags route to rotation, pan, and zoom handlers; input is
  dropped during a pending fade/loading transition. No persistent failure after gameplay settles
  has been established. See [CAMERA-INPUT-FINDING.md](CAMERA-INPUT-FINDING.md).
- [x] Record that desktop and browser builds compiled for the existing menu-scene slice. This is
  not an interactive verification of the new-game camera or full loop.

## Remaining reverse-engineering work

- [ ] Resolve the four `FUN_1400c7380` callers (`FUN_1400e9660`, `FUN_1400eb3b0`, `FUN_1400f0390`,
  and `FUN_1400f3880`) to named resource/job transitions and determine what selects the
  Remove/Animate sequence.
- [ ] Find producers of `NaturalResourceComponent` byte `+0x2d` and consumers of the category
  counter at `+0x10`; correlate those fields and the map-cell transition bytes with a decoded save
  before assigning gameplay meanings.
- [x] Trace town storage search, shared inventory ledgers, reservations and New Game tool/clothing
  fetch sequences. Verify search/geometry/ledger/equipment messages against original functions;
  see [CITIZEN-AI.md](CITIZEN-AI.md#storage-loose-piles-and-inventories).
- [ ] Complete production, harvest, storage, and stock updates from resource entities through the
  Status Bar counters. Existing status flags and fixed-stride stock layout are only partially
  identified; see steps 8-9.
- [ ] Complete start-condition application beyond the traced structures, household creation,
  inventory and natural-resource seeding: residence assignment, wildlife and other populations.
- [ ] Complete climate precipitation geometry/effects and ambient audio beyond the traced and
  ported temperature, weather-state, threshold and seasonal controllers.
- [ ] Extend beyond natural resources into citizen/job scheduling, movement, needs, production,
  consumption, events, and entity removal.

## Remaining port work

- [x] Live Status Bar population, stock, StatusIcon mask counts and scaled whole/half health and
  happiness meters; 1,000 original-function cases and a UI read-only integration check. See
  [UTILITY-WINDOWS.md](UTILITY-WINDOWS.md#live-status-bar-slice-2026-10-15).

- [x] Make climate selection drive the environment controller's temperatures and weather draws.
- [x] Seed the starting map from the selected profile: start point, structure draws, and rock, ore
  and tree seeding with spawned herbs, mushrooms and berries. Rendered in game. See
  [RESOURCE-SEEDING.md](RESOURCE-SEEDING.md).
- [x] Starting goods into entity-associated storage ledgers, with material counts, reservations,
  masks and capacity; shown in the Status Bar. See
  [STARTING-INVENTORY-DISTRIBUTION.md](STARTING-INVENTORY-DISTRIBUTION.md).
- [x] Starting households (couples, children, names, ages, lifespans), shown as Status Bar
  population. Port the exact name generator, verified bit-exact under emulation. See
  [CITIZEN-NEEDS-AND-HOUSEHOLDS.md](CITIZEN-NEEDS-AND-HOUSEHOLDS.md) and
  [TOWN-NAME-DATABASE.md](TOWN-NAME-DATABASE.md).
- [x] Draw starting citizen meshes in bind pose; animation and movement remain below.
- [x] Citizen movement/action stacks, loitering, sector-ranked storage search, reserved tool/clothing
  fetching and equipment messages at New Game. Original-function parity and seeded integration
  checks are recorded in [CITIZEN-AI.md](CITIZEN-AI.md).
- [ ] Complete citizen activities/jobs/needs and bucket scheduling; deer, crops, orchards, livestock,
  building decals and footprints.
- [x] Replace the compact HUD with the packaged toolbar and Status Bar controllers. See
  [IN-GAME-TOOLBAR.md](IN-GAME-TOOLBAR.md) and [UTILITY-WINDOWS.md](UTILITY-WINDOWS.md).
- [x] Building placement tool: ghost, rotation, variants, original validity and placement of
  construction sites. See [BUILD-TOOLS.md](BUILD-TOOLS.md).
- [ ] Add drag/road/removal tools, construction progress, object selection, the remaining utility-window controllers (Event Log,
  Professions, Map, …), and the original pause/options dialogs.
- [x] Port the environment controller (temperature, weather state, level, snow/autumn/leaf values)
  and run it per simulation iteration after the calendar. See [ENVIRONMENT.md](ENVIRONMENT.md).
- [x] Connect seeded natural resources to scheduled growth, periodic same-template spawning,
  map transitions and non-tree removal, with one post-iteration smoothing pass and live GPU
  instance updates. Tree expiry animation/completion is covered by step 19 below.
  See [GAME-LOOP-STEP-18.md](GAME-LOOP-STEP-18.md).
- [x] Play tree expiry animation index 2 from its packaged group, update each instance's rigid
  node pose, and remove the entity/map stamp at the serialized completion tick; verify on GPU.
  See [GAME-LOOP-STEP-19.md](GAME-LOOP-STEP-19.md).
- [ ] Port ongoing SpawnComponent cross-template spawning and spatial AnimAudioEvent/SoundEffect
  dispatch. Shared/blended animation states also remain open.
- [x] GPU-skin citizens with a per-instance bone palette (`MeshInstance::GetBones`), playing each
  citizen's `Idle` clip; shared with the viewer (`banished_gfx::skin`). See
  [CITIZEN-NEEDS-AND-HOUSEHOLDS.md](CITIZEN-NEEDS-AND-HOUSEHOLDS.md#skinning).
- [ ] Port weather effects (rain/snow geometry, ambient audio, timed spawn) and connect weather to
  citizens, production, events, and resource consumption.
- [ ] Add save/load, autosave, and event notifications to the active gameplay loop.
- [ ] Verify the new-game flow in browser and desktop builds, including camera behavior after the
  fade. Capture the failing platform and gesture, then confirm camera rotation, middle-drag pan,
  wheel zoom, and keyboard movement.

## Evidence and verification limits

- The original startup and resource/action findings are static analysis of the shipped x64
  executable, Ghidra export, class registrations, vtables, and package data. They establish code
  paths and serialized fields, not observed timing or player-visible semantics.
- A previous source audit found that the loading handoff can discard input while a transition is
  pending. It does not explain a camera that remains unresponsive after gameplay has settled; that
  needs a runtime reproduction with the affected platform and gesture recorded.
- Step 18 records a passed full workspace test run (two existing save tests ignored), a release
  WASM build, original-function growth/scheduler parity, a generated-town GPU integration test,
  and an inspected native New Game capture. Interactive browser/camera verification and
  original-game transition/visual parity remain open.
- Step 19 corrects time units and entity/calendar/environment order, proves animation poses and
  unique playback against original functions, and records inspected native tree-expiry captures.
  These are bounded original-function and remake tests; the complete game is not yet verified.
- The storage/equipment slice verifies 600 storage searches, 240 geometry queries, 240 facing angles,
  400 ledger/action cases and 300 equipment messages against isolated original functions. The seeded New Game
  integration checks equipment consumption, return to loitering, exhausted-storage failures and
  reservation cancellation. Full original-game timing/visual parity remains unverified; the
  explicit current deviation list is in CITIZEN-AI.md. Workspace tests: 99 passed, two existing
  save tests ignored; release WASM builds; the inspected seed-1234 22-second capture is
  deterministic across two runs.
