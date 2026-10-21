# Performance

This document covers how to measure Un-Banished's performance, what has been fixed, and where
the port can still become cheaper in memory, CPU and GPU without changing what the player sees.
Evidence is measured: frame-phase timings from the built-in profiler and sampling profiles
from samply, on a MacBook Pro with an Apple M3 and 96 GB of memory, release builds. The rule from
[AGENTS.md](../AGENTS.md) applies: 1:1 behaviour stays; only the implementation gets cheaper.

## Tools

### Frame profiler (`UNBANISHED_PROFILE`)

`apps/game/src/platform/perf.rs` times named phases of every frame. It is off by default and costs one
flag check per phase when off.

```bash
UNBANISHED_PROFILE=1 cargo run --release -p unbanished-app          # hitches and summaries
UNBANISHED_PROFILE=hitches cargo run --release -p unbanished-app    # hitches only
```

In the browser, add `?profile` to the page address. Headless captures work too, and draw
every frame while profiling:

```bash
UNBANISHED_PROFILE=1 target/release/unbanished --boot-capture /tmp/p.png 75 \
  --script "load path/to/Save.sav@4.5"
```

- **Hitch:** a frame longer than twice the recent median and longer than 25 ms. The log line
  names the phases that took the time, for example
  `hitch: frame 333 took 48.9 ms (median 4.3): draw.update_instances 44.6, ...`.
- **Summary:** every 600 frames, each phase's median, 95th percentile and worst time.

`UNBANISHED_THREADS=1` makes loading use one thread (the default is one per core), to compare
the parallel loading with the sequential one.

The phases are `update`, `draw`, `sim.iterate`, `sim.town`, `sim.citizens`, `smooth`,
`refresh_models`, `refresh_decals`, `refresh_particles`, `refresh_weather`, `audio`,
`draw.set_scene`, `draw.update_instances`, `draw.upload_decals` and `draw.gpu`. `draw.gpu` is
the time the GPU takes for the frame: while profiling (native only), the renderer waits for the
GPU after each frame, and a headless capture draws each frame into a scratch target so that the
GPU work happens as in a window. Add more with
`let _p = crate::perf::scope("name");` at the start of a block.

### Sampling profiler (samply)

[samply](https://github.com/mstange/samply) samples the whole program and shows where the time
goes, function by function. Build with line tables in a separate target folder, record, then
summarise or open the profile in the Firefox Profiler:

```bash
CARGO_TARGET_DIR=target/profiling CARGO_PROFILE_RELEASE_DEBUG=line-tables-only \
  cargo build --release -p unbanished-app
samply record --save-only --unstable-presymbolicate --rate 2000 -o /tmp/run.json.gz -- \
  target/profiling/release/unbanished --boot-capture /tmp/p.png 60 --script "load SAVE@4.5"
python3 tools/perf/samply_top.py /tmp/run.json.gz --after 6 --top 30   # skip loading
samply load /tmp/run.json.gz                                           # interactive view
```

`samply_top.py` also takes `--grep FUNCTION` (only stacks through it) and `--callers FUNCTION`.

### Browser

`tools/firefox-check.mjs` drives Firefox over WebDriver BiDi: console with stack traces, clicks,
screenshots, and the Web Audio output level. For a readable stack from a wasm abort, build with
function names (see [BROWSER.md](BROWSER.md)).

## Fixed (2026-10-19)

All measured on the player save from issue #5 (Herstower, 17 citizens), 70 s after loading.

| Problem | Cause | Fix | Before | After |
|---|---|---|---|---|
| A 45–55 ms hitch every few seconds (issue #5) | When a new kind of model appeared, `ModelRenderer::update_instances` re-uploaded every group and every texture | Upload only the new groups; keep texture views for the scene | 55 ms worst, 5 hitches per 10 s | 2.6 ms worst, 0 hitches |
| `Ambience::reweigh`: 19 % of frame CPU | Two string copies per cell per terrain class, each time models changed (almost every frame) | Match each template once per call, without copies | 1.30 ms `audio` median | 0.68 ms |
| Achievement checks: 13 % of frame CPU | `same()` lowercased two strings per entity per goal per frame | Compare in place (`eq_ignore_ascii_case`) | | (out of the top list) |
| Out of memory in the WebGL build (issue #7) | Each decal held its own copy of its textures | `DecalCache` shares one copy per texture | 1.14 GB | 24 MB |
| Duplicate model textures | Each mesh range held its own copy; the cache was rebuilt per update | `SceneModels` keeps one shared copy per texture for the scene | 101 copies, 17.6 MB | 69 images, 7.1 MB (and the GPU copies) |

Frame time overall: median 4.3 → 3.7 ms, 95th percentile 14.7 → 5.4 ms, worst steady-state
frame 56 → 6.1 ms. `determinism.sh` gives identical frames before and after.

## Large towns (2026-10-19)

Measured on `save-game/Auto-1.sav` (Driscola: 1,024 citizens, 28,000 placed entities, made in
the original game in 2015) at speed 10. A capture frame is two 1/60 s updates.

| Change | Cause | Fix |
|---|---|---|
| `entity_views` (38 % of time) | A view of all 28,000 entities, about twice per iteration, ran an iterator chain even for entities with no use points, and looked each entity's site up in a tree | Skip the chain for those entities; walk the sites alongside the entities |
| `sync_map` (16 %) | Compared the whole 512 × 512 type map about six times per iteration, and refilled the path regions after any type change | A revision stamp on the types (`Cells::types_revision`) skips the comparison; the region fill runs only when walkability changed |
| `Floors::ground_normal` (11 %) | Tested every bridge floor for every character on a bridge cell | Skip floors whose footprint misses the downward ray |
| `refresh_models` (7 ms per update) | Hashed a copied mesh name for every entity every frame | Find each entity's group through a small table per template |

| | Before | After |
|---|---|---|
| Frame, median | 94.3 ms | 43.0 ms |
| `sim.iterate` | 78 ms | 29 ms |
| `sim.citizens` | 50 ms | 21 ms |
| `refresh_models` | 7.2 ms | 4.3 ms |

Every change was checked with `tools/compat/compare.sh` and `tools/triage/same-run.sh`: the same
save, played 40 s at speed 10, gives identical game state (33 MB) and frames before and after.
[COMPATIBILITY.md](COMPATIBILITY.md) describes the method.

Still the largest costs per update on this town: `refresh_models` (4.3 ms), `audio` (3.4 ms),
`refresh_particles` (2.7 ms), and in the simulation the characters' movement and path finding.

## Drawing less (2026-10-19)

Measured on the large town above with `UNBANISHED_PROFILE=1` and the new `draw.gpu` phase, at
1280 × 800. The GPU took 22.7 ms per frame (median). Every model and every terrain tile was
drawn in all three passes: the sun's shadow map, the water reflection and the scene. A frame
with one part left out at a time showed where the time went:

| Part | GPU time |
|---|---|
| The terrain in the water reflection | 8.5 ms |
| The shadow pass (terrain and models) | 3.9 ms |
| Decals | 2.8 ms |
| Models in the scene | 2.4 ms |
| The terrain in the scene | 2.3 ms |
| Water | 1.2 ms |

Changes (`gfx/models/culling.rs`, `gfx/render/frame.rs`):

- **Models:** each group has a bounding sphere, made from its vertices and padded for what the
  vertex shaders add (wind, leaf cards, skinned poses). Before each pass, the renderer tests every
  instance's sphere against that pass's view and draws only the instances that can reach it, in
  their original order. In the scene pass of this town, 2,684 of 25,788 models are drawn; in the
  shadow pass 11,244; in the reflection (at ReflectionDetail 2) 1,500 to 3,400.
- **Terrain:** each 32 × 32-cell tile has a box. A pass skips the tiles outside its view. The
  reflection also skips tiles that lie wholly below the water, because its shader discards
  everything below the water.

An instance or tile is skipped only when none of its triangles can reach the pass's target, so
the frames do not change. This was checked with `tools/compat/compare.sh` (all scenarios) and
with a camera tour (pan and zoom, eight screenshots per town, with the default and the highest
video settings) on the medium and large towns. See [the open question](#open-questions) on the
frames that differ from run to run.

| Same machine, alternate runs | Before | After |
|---|---|---|
| GPU time per frame (median) | 31.8 / 32.6 ms | 17.5 / 16.6 ms |
| Frame at speed 10 (median) | 86.6 / 85.7 ms | 69.5 / 63.5 ms |

The GPU time is now about half. (These runs were on a warmer machine than the 22.7 ms
measurement; compare the columns, not the runs.)

### Levels of detail

A `GraphicsMesh` can hold several detail levels (parts), each with the distance up to which it
is used (`GraphicsMesh::GetDetailLevelDistance`, 0x180012f40: the part's `+0x10`). The port
loads every level and picks one per instance and per pass, as `MeshInstance::AddDrawItems`
(0x1801037b0) does (`detail_level` in `apps/game/src/gfx/models/culling.rs`, called from
`ModelRenderer::cull`).

| Step | Observed in the original |
|---|---|
| Which meshes | Only a mesh with more than one part (`GraphicsMesh +0x58`); the others always draw part 0 |
| The box | The union of the instance's current world box (`Geometry +0x4c`) and its previous one (`+0x64`, copied there by `Geometry::Clean`, 0x1800a5370). The current box is the mesh box (`GraphicsMesh +0x80`/`+0x8c`, the last fields `GraphicsMesh::Serialize` writes) moved by the instance's rotation and position (`FUN_18001fd50`) |
| The point | The box centre, `(min + max) × 0.5` (`DAT_180164804` = 0.5) |
| The camera | The current draw group's camera (`Renderer +0x10`, group stride 0x1f0, index `+0x34`), at group `+0x90`: the camera (group `+0x8`) at `+0x88`, the position in its camera-to-world (`Camera +0x64`, `GetCameraToWorld` 0x1800186f0). Not the cull camera (group `+0xe0`) |
| The choice | The first part whose distance squared is larger than the squared distance (strictly); if none is, part 0 |

The cameras of the three passes:

- **Scene**: the camera's eye.
- **Water reflection**: `ReflectionImageBuffer::Render` (0x1800a7ad0) multiplies the camera's
  camera-to-world by the reflection in the water plane, so the distance is measured from the
  mirrored eye (the eye with its height mirrored in the water level).
- **Shadow map**: `SunLight::AddDrawItems` (0x1801097e0) begins the sun's draw group with the
  sun camera only as its cull camera; its camera is group 0's. So the shadow map measures from
  the scene's eye too, not from the sun (*inferred*: that group 0 is the scene's group). Rocks
  and ore therefore cast the shadow of the level they are drawn at.

In the shipped data only 6 of 271 meshes have a second level: `Models/Rocks/Rock01`–`03Mesh`
and `Models/NaturalResource/Ore/Ore01`–`03Mesh`. Their first level is used up to 50 to 70
units (Rock01 50, Ore02 65), then a `_lod` part with 20 to 85 % fewer triangles up to 10,000
units. Every other mesh has one level, used up to 10,000. A frame changes only where one of
these rocks or ores is further than its switch distance.

Differences that cannot be seen: the port measures from the current box only (a
"Deviation:" in `culling.rs`; rocks and ore never move, so both boxes are the same), and it
leaves the instance's visual scale out of the box (*inferred*: the original's transform has no
scale; rocks and ore are drawn at scale 1).

Open: what `GraphicsMesh::GetMaxDepthDistance` (0x180012f60, `+0x48`) is used for.

Also tried: rebuilding a model group's instances only when its placements change. On a
running town almost every group changes every frame, because the growth smoothing changes the
trees' scales, so the comparison cost more than it saved. It was not kept.

## Frame rate of a mid-sized town (2026-10-20)

Goldfoothill is a vanilla player save (no mods): year 83, 565 citizens, 371 buildings (164
wood houses, 62 storage barns, 24 fields, 23 iron mines), 433 animals. It is kept locally as
`save-game/Goldfoothill.sav` (saves are never committed). The target is 60 frames a second.

**Browser, measured.** The game loaded from the start page's Load dialog through the
[probe](develop/PROBE.md), in a 1280×720 page; frames counted with `requestAnimationFrame` for
20 s after 11 s of play. Apple M3, headless Chrome (WebGPU) and Firefox Nightly 160 (WebGL 2).

| Game speed | Chrome fps | Chrome frames over 20 ms | Firefox fps |
|---|---|---|---|
| 1 | 59.9 | 3 of 1197 | 58.3 |
| 5 | 54.5 | 111 of 1089 | 28.2 |
| 10 | 28.1 | 485 of 562 | 23.3 |

The main menu runs at a steady 60 in both.

**Desktop, profiled.** Headless captures at 1920×1080 with `UNBANISHED_PROFILE=1`, 80 s, the
second and third 600-frame summaries (after loading). The profiler waits for the GPU every
frame, so its frame time is CPU and GPU added up; a window overlaps them. Per 1/60 s update
(the profiler's per-frame sums halved, since a capture frame runs two updates):

| Phase | Speed 1 | Speed 10 |
|---|---|---|
| `sim.iterate` (all of the simulation) | 1.0 ms | 8.3 ms |
| of which `sim.citizens` | 0.6 ms | 5.2 ms |
| `refresh_models` + `audio` + `refresh_particles` | 4.0 ms | 4.1 ms |
| `draw` on the CPU (without the GPU) | 3.2 ms | 3.2 ms |
| `draw.gpu` | 6.5 ms | 9.7 ms |
| One update and one draw, GPU waited for | 15.0 ms (66 fps) | 25.7 ms (39 fps) |
| The same with CPU and GPU overlapped (estimate) | about 117 fps | about 62 fps |

Speed 5 lies between: 19.5 ms (51 fps) with the GPU waited for, about 83 fps overlapped. The
highest video settings change little here (66 → 67 fps waited for).

So the desktop holds 60 at speeds 1 and 5 with room to spare, and is near 60 at speed 10. The
browser holds 60 only at speed 1. At speed 10 the simulation (most of it the citizens) takes
half a 60 fps frame even natively; the browser runs it on one thread in WebAssembly, and drops
to 28 fps in Chrome. The scene refresh (models, ambience, particles) costs 4 ms every update
whatever the speed, and is the next largest CPU cost.

## Where the time goes in a 1,024-citizen town (2026-10-20)

Driscola (`save-game/Save200.sav`, 1,024 citizens, 650 buildings), a headless capture at
1920 × 1080 with the default video settings and `UNBANISHED_PROFILE=1`, 62 s; the third 600-frame
summary. The summary's phase times are sums per capture frame (two updates and one draw); the
update phases below are halved to one update, the draw is per picture.

| Phase | Speed 1 | Speed 10 |
|---|---|---|
| `sim.iterate` | 1.8 ms | 15.9 ms |
| of which `sim.citizens` | 1.2 ms | 11.7 ms |
| of which `sim.town` | 0.4 ms | 2.2 ms |
| `refresh_models` + `audio` + `refresh_particles` | 5.7 ms | 5.5 ms |
| `draw` on the CPU (without `draw.gpu`) | 4.3 ms | 4.1 ms |
| `draw.gpu` | 9.9 ms | 15.3 ms |
| Capture frame, median | 30.3 ms | 64.2 ms |

At speed 1 the scene refresh costs about three times the simulation; at speed 10 the
simulation alone is about one 60 fps frame. These agree with the frame rates measured the same
day: Driscola at 58 / 40 / 26 fps on the desktop with the GPU waited for (86 / 56 / 40 with CPU
and GPU overlapped), and 60 / 26 / 22 fps in Chrome at speeds 1 / 5 / 10. The blog post
[Sixty a second](https://tanepiper.github.io/unbanished/blog/measuring-speed) presents these results for players.

## Opportunities

Ranked by expected gain against risk to 1:1 behaviour. "Measured" figures come from the samply
profile of the same save after the fixes above (share of main-thread CPU after loading).

### 1. Rebuild model instances only when something changed (measured: about 25 %; see "Drawing less")

`StartingTown::model_instances` (16 %) rebuilds every instance of every model each frame,
grouping them in a `HashMap<(String, GroupMaterial), Vec<…>>` keyed by strings (SipHash shows
as 6 % self time). Then `queue.write_buffer` uploads every group's instances (7 %).

- Keep a dirty flag per placed entity (moved, grown, animated, added, removed) and rebuild only
  the affected groups. Key groups by a template index instead of a string.
- Upload only dirty groups. Trees and buildings that do not change need no upload at all.
- Risk: low. Rendering only; the simulation does not read these lists.

### 2. Cache entity views (measured: about 11 %)

`agents::entity_views` builds a view of every placed entity every update for the citizens' AI.
Most fields do not change between updates.

- Keep the views and update them when an entity changes, or build them once per update and
  share them between the users.
- Risk: medium. The AI reads these views, so the update order must stay exactly as now.
  `determinism.sh` and the emulator tests must pass.

### 3. Move loading work off the first frame (done: 490 ms → under 25 ms)

Measured on `save-game/Save83.sav` (Fort Ponchartrain, 13 677 placed entities) with
`UNBANISHED_PROFILE=1` and samply. The first gameplay frame took 490 ms: `audio` 417 ms,
`draw.upload_decals` 38 ms, `refresh_decals` 19 ms. samply showed that almost all of `audio`
was WMA decoding in `EffectCache::get`, not the emitters themselves:

| Work in the first frame | Time | Now done in |
|---|---|---|
| Decoding the ambience's sounds (`Ambience::create`: terrain effects, river, environment, wind, and the buildings' `ambientemitter` slots) | about 165 ms | `ambience::preload_effects`, from `GameplayLoop::load` |
| Decoding the first music track (`MusicPlayer::update`) | about 250 ms | `MusicPlayer::first_track`, from `GameplayLoop::load` |
| Building the decals (`refresh_decals`) | 19 ms | `GameplayLoop::load` |
| Uploading the decals (`draw.upload_decals`) | 38 ms | the world upload (`draw.set_scene`) of the frame that switches loops, while the screen is black |

`GameplayLoop::load` runs while the loading dialog shows (`DialogLoaderLoop`). The preloads
only fill the effect cache: nothing plays, and the emitters and the music still start on the
first gameplay frame, as before. The music preload predicts the track with a copy of the audio
stream's Random (`Audio::peek_rand_below`) and the month and mood at load time; if the first
frame's choice differs, that frame decodes its own choice, as before. The decals are rebuilt on
the first frame only if the town changed since the load. Since item 4, the loading screen
decodes all these sounds in one batch on several threads, while the decals are built.

Result: no hitch on the first frames after the load (every frame under the 25 ms threshold).
The loading frame takes about 330 ms longer, behind the loading dialog, and the switch frame
(black) 26 ms longer. Captures are byte-identical before and after, and `determinism.sh 22 1234`
gives identical frames.

### 4. Use more than one core for loading (native) (done)

Measured with samply (`--before` on the load window) and `UNBANISHED_PROFILE=hitches`, with the
headless captures of `tools/triage/determinism.sh` (seed 1234) and `save-game/Save200.sav`
(Driscola, a 15 MB save). Before the change, loading ran on one thread:

| Where the time went | Share | What it is |
|---|---|---|
| Startup (`Registry::build`) | about 200 of 335 ms | reading and scanning every resource the game's root list reaches (LZF decompression) |
| Menu load | about 165 of 275 ms | decoding the menu music (WMA) |
| First menu frame (`draw.set_scene`) | about 70 of 112 ms | DXT to RGBA decoding of the scene's textures (`decode_rgba`) |
| New Game load | 1 040 ms | `Generator::stage4_downhill` (terrain, uses `Random`, cannot be split) |
| | about 200 ms | `Generator::surface_map` (terrain surface image) |
| | about 335 ms | decoding the ambience's sounds and the first music track (item 3) |
| Save load | about 600 ms | `StartingTown::from_save`, mostly `SavedGame::component` (one thread, see below) |
| | about 335 ms | the same sound decoding |
| | about 65 ms | meshes and their textures (`SceneModels::from_instances`) |

`unbanished_formats::par` runs work that is a pure function of one item on several threads with
`std::thread::scope` (no new dependency). Results come back in input order, so the output is
the same as from one thread. Where it is used:

- `Registry::build`: each wave of newly found resources is read and scanned in parallel; then
  the original depth-first walk runs over the results. A test compares it with the walk that
  reads one resource at a time (`the_registry_matches_a_walk_in_order`).
- `decode_rgba`: levels of 512 × 512 texels or more decode in bands of 8 block rows. A test
  compares the bands with one pass (`banded_decode_matches_one_pass`).
- `EffectCache::prepare`: all the sound buffers of all the effects asked for decode in
  parallel. One WMA stream cannot be split (the decoder carries the noise table index, the
  previous exponents and the bit reservoir from frame to frame), so the long music tracks run
  next to other work instead: the menu music decodes while the menu scene loads, and the
  gameplay sounds decode while the decals are built (`par::join`).
- `SceneModels::from_menu_scene` and `from_instances`: the meshes, then the textures not in the
  cache, are read in parallel; then the groups take their textures from the cache in order.
- `Generator::surface_map`: the two `Random` draws come first, as before; then every texel is
  a function of the finished height grid, so the rows are computed in parallel. The emulator
  test (`TERRAIN_STAGES=5 cargo test -p unbanished-game --test terrain_emu`) gives a bit-exact
  surface image for all four emulated maps.

Nothing that draws from `Random` or depends on the order of work runs in parallel: the terrain
stages, seeding and the simulation are unchanged. `UNBANISHED_THREADS=1` runs everything on one
thread and gives the same times as before the change.

Results, median of five runs, milliseconds (the test machine has 14 cores: 10 performance, 4
efficiency):

| Step | Before | After |
|---|---|---|
| Start to the loader loop (packages, registry, renderer) | 335 | 146 |
| Menu load (frame 71) | 276 | 172 |
| First menu frame, texture uploads (frame 93) | 112 | 81 |
| New Game, loading frame (`World::generate` and the town) | 1 853 | 1 497 |
| Loading `Save200.sav`, loading frame | 1 067 | 790 |

"Before" is commit 46f9a2d (item 3 included, which moved about 335 ms of sound decoding into
these loading frames). The menu, New Game (seeds 1234 and 4321, 22 s) and save captures are
byte-identical to it, and `determinism.sh 22 1234` gives identical frames.

What is left:

- `stage4_downhill` (about 1 s on a small map) moves one drop at a time and each drop reads
  the heights the previous one wrote: it cannot run in parallel and stay 1:1.
- Done after this item: `SavedGame::component` read the entity's `entity.components` field
  (a search by name), looked up every component by number and compared type names, on each of
  several hundred thousand calls while a save loads. `SavedGame` now builds a table once, on
  the first lookup: per object number, the object's address and its components as (type name,
  number), in the same order. A lookup is then a slice and a short scan. An object that is not
  the save's own (a copy), or a save whose numbers are too sparse for the table, still reads
  the field. A test compares the table with the field for every object of every save in
  `save-game/` (about 735 000 components). Loading frame, median of five, before (707cfea) →
  after: `Save200.sav` 827 → 536 ms, `Save83.sav` 459 → 363 ms. The captures of three saves
  match the build before (Save200's differ by one level between runs of either build).
- The first menu frame after the menu load opens the audio output (rodio and CoreAudio,
  about 85 ms, frame 94).
- Browser: wasm threads need `SharedArrayBuffer`, which needs cross-origin isolation headers
  (COOP/COEP). GitHub Pages cannot set headers; a service worker can add them. The wasm32
  build runs `par` in order on the calling thread.

### 5. Simulation on its own thread (only at high game speed)

At normal speed the simulation (`sim.iterate`) takes about 0.9 ms a frame; at the highest speed
it runs many iterations per frame. The simulation itself must stay single-threaded and in order
(`Random` order, iteration order). What can move is the work around it: building render lists,
particles, ambience and UI could run on another thread while the next simulation step runs.

- Measure at the highest speed first; do this only if the simulation dominates there.
- Risk: medium (data shared between threads), none for the rules if the simulation is untouched.

### 6. Faster hashing for internal maps

SipHash (Rust's default, resistant to hash-flooding attacks) shows as 6 % self time. Internal maps
keyed by integers or the game's own strings do not need that protection.

- Use a faster hasher (for example `rustc-hash`'s `FxHashMap`) for those maps.
- Risk: low. The default hasher already uses a random order per run, so no deterministic
  result can depend on a `HashMap`'s order (`determinism.sh` would fail). Check each changed map
  anyway.

### 7. Memory: game packages

The four `.pkg` files (165 MB) stay in memory as byte vectors for the whole session, next to
the decoded data made from them.

- Native: memory-map the packages (`memmap2`) so the operating system pages them in and out.
- Browser: the files must be read into memory, but decoded data that is no longer needed (menu
  scenes after the menu) can be dropped.
- Risk: low.

### 8. Per-frame copies in `Renderer::set_scene`

`set_scene` copies the status icons, weather and overlay lists from the scene view every frame.

- Pass them by reference or swap buffers when they change.
- Risk: low; small gain (about 0.1 ms).

## Open questions

- How large are the costs on a large town (300+ citizens)? The measurements above use a small
  town; costs that grow with entities (items 1 and 2) will matter more there.
- What does the highest game speed cost per frame, and how much of it is the simulation?
- The decals (2.8 ms of GPU time on the large town) and the shadow pass are the next largest
  GPU costs. Can the decals be culled per pass in the same way?
- How much GPU memory do the textures use after decoding, per platform?
