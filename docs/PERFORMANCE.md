# Performance

This document covers how to measure Un-Banished's performance, what has been fixed, and where
the port can still become cheaper in memory, CPU and GPU without changing what the player sees.
Evidence is measured: frame-phase timings from the built-in profiler and sampling profiles
from samply, on a MacBook Pro with an Apple M3 and 96 GB of memory, release builds. The rule from
[AGENTS.md](../AGENTS.md) applies: 1:1 behaviour stays; only the implementation gets cheaper.

## Tools

### Frame profiler (`BANISHED_PROFILE`)

`apps/game/src/perf.rs` times named phases of every frame. It is off by default and costs one
flag check per phase when off.

```bash
BANISHED_PROFILE=1 cargo run --release -p banished-app          # hitches and summaries
BANISHED_PROFILE=hitches cargo run --release -p banished-app    # hitches only
```

In the browser, add `?profile` to the page address. Headless captures work too, and draw
every frame while profiling:

```bash
BANISHED_PROFILE=1 target/release/banished --boot-capture /tmp/p.png 75 \
  --script "load path/to/Save.sav@4.5"
```

- **Hitch:** a frame longer than twice the recent median and longer than 25 ms. The log line
  names the phases that took the time, for example
  `hitch: frame 333 took 48.9 ms (median 4.3): draw.update_instances 44.6, ...`.
- **Summary:** every 600 frames, each phase's median, 95th percentile and worst time.

`BANISHED_THREADS=1` makes loading use one thread (the default is one per core), to compare
the parallel loading with the sequential one.

The phases are `update`, `draw`, `sim.iterate`, `sim.town`, `sim.citizens`, `smooth`,
`refresh_models`, `refresh_decals`, `refresh_particles`, `refresh_weather`, `audio`,
`draw.set_scene`, `draw.update_instances` and `draw.upload_decals`. Add more with
`let _p = crate::perf::scope("name");` at the start of a block.

### Sampling profiler (samply)

[samply](https://github.com/mstange/samply) samples the whole program and shows where the time
goes, function by function. Build with line tables in a separate target folder, record, then
summarise or open the profile in the Firefox Profiler:

```bash
CARGO_TARGET_DIR=target/profiling CARGO_PROFILE_RELEASE_DEBUG=line-tables-only \
  cargo build --release -p banished-app
samply record --save-only --unstable-presymbolicate --rate 2000 -o /tmp/run.json.gz -- \
  target/profiling/release/banished --boot-capture /tmp/p.png 60 --script "load SAVE@4.5"
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

## Opportunities

Ranked by expected gain against risk to 1:1 behaviour. "Measured" figures come from the samply
profile of the same save after the fixes above (share of main-thread CPU after loading).

### 1. Rebuild model instances only when something changed (measured: about 25 %)

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
`BANISHED_PROFILE=1` and samply. The first gameplay frame took 490 ms: `audio` 417 ms,
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

Measured with samply (`--before` on the load window) and `BANISHED_PROFILE=hitches`, with the
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

`banished_formats::par` runs work that is a pure function of one item on several threads with
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
  test (`TERRAIN_STAGES=5 cargo test -p banished-game --test terrain_emu`) gives a bit-exact
  surface image for all four emulated maps.

Nothing that draws from `Random` or depends on the order of work runs in parallel: the terrain
stages, seeding and the simulation are unchanged. `BANISHED_THREADS=1` runs everything on one
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
- `SavedGame::component` looks up an entity's components through its field list on every
  call; caching the lookup would save most of 600 ms on a large save. That is single-thread
  work, not parallelism.
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
- How much GPU memory do the textures use after decoding, per platform?
