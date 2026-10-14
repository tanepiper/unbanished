# Game loop reverse-engineering and port checklist

Updated 2026-10-14. This checklist tracks the gap between the new-game map view and a complete,
interactive Banished game loop. The linked step documents hold the supporting Ghidra/resource
evidence; static traces are not claims of live-game behavior.

## Completed traces and current implementation

- [x] Trace the New Game handoff: town name, seed, terrain size, selected map/terrain/climate
  profiles, fresh-game setup, and transition into gameplay. See
  [GAMEPLAY-LOOP-FOUNDATIONS.md](GAMEPLAY-LOOP-FOUNDATIONS.md),
  [WORLD-GENERATION.md](WORLD-GENERATION.md), and
  [START-CONDITION-PROFILES.md](START-CONDITION-PROFILES.md).
- [x] Implement an interactive generated map, compact HUD, date display, pause/speed controls,
  camera rotation, pan and zoom, and monthly environment lighting. The selected climate and
  starting-condition labels currently display, but do not yet create their game effects or entities.
- [x] Trace the outer loop, calendar, climate temperature/weather thresholds, and packaged gameplay
  dialogs. See [GAME-LOOP-STEP-01.md](GAME-LOOP-STEP-01.md) through
  [GAME-LOOP-STEP-10.md](GAME-LOOP-STEP-10.md).
- [x] Trace the first scheduled natural-resource component: growth smoothing, timed child placement,
  neighbor-density gate, map-cell admission, and shipped resource-description branches. See
  [GAME-LOOP-STEP-11.md](GAME-LOOP-STEP-11.md) through
  [GAME-LOOP-STEP-14.md](GAME-LOOP-STEP-14.md).
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
- [ ] Trace production, harvest, storage, and stock updates from resource entities through the
  Status Bar counters. Existing status flags and fixed-stride stock layout are only partially
  identified; see steps 8-9.
- [ ] Trace start-condition application from `FUN_1400a87d0`: initial structures, households,
  inventories, raw materials, and natural-resource seeding.
- [ ] Trace how the selected climate profile changes temperatures, annual weather thresholds,
  weather transitions, precipitation/snow, effects, and audio. Only climate configuration and
  threshold setup are currently mapped.
- [ ] Extend beyond natural resources into citizen/job scheduling, movement, needs, production,
  consumption, events, and entity removal.

## Remaining port work

- [x] Make climate selection drive the environment controller's temperatures and weather draws.
- [ ] Apply the selected starting condition and seed the correct starting entities and inventories.
- [x] Replace the compact HUD with the packaged toolbar and Status Bar controllers. See
  [IN-GAME-TOOLBAR.md](IN-GAME-TOOLBAR.md) and [UTILITY-WINDOWS.md](UTILITY-WINDOWS.md).
- [ ] Add build/tool modes, object selection, the remaining utility-window controllers (Event Log,
  Professions, Map, …), and the original pause/options dialogs.
- [x] Port the environment controller (temperature, weather state, level, snow/autumn/leaf values)
  and run it per simulation iteration after the calendar. See [ENVIRONMENT.md](ENVIRONMENT.md).
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
- The foundation note records prior desktop and `wasm32` compilation. No interactive runtime
  capture of the new-game loop is recorded, and no tests were run for this checklist update.
