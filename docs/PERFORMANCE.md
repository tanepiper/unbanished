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

### 3. Move loading work off the first frame (measured: about 0.4 s once)

The first frame after a load spends 384 ms creating the ambient sound emitters
(`Ambience::create`, sound decoding) and 34 ms uploading decals. The player sees a freeze as the
town appears.

- Do this work behind the loading screen, or spread it over the first frames.
- Risk: low, if the emitters start at the same game time as now.

### 4. Use more than one core for loading (native)

Loading decodes textures (DXT to RGBA), WMA audio and meshes on one thread, and generates the
terrain (3.5 s for a small map in the browser).

- Decode textures and sounds in parallel (for example with `rayon`): each decode is a pure
  function of its file, so the result is identical.
- Terrain generation uses the game's `Random` in a fixed order, so only parts that do not draw
  random numbers can run in parallel, and only if the result stays bit-identical.
- Risk: low for decoding; high for anything that uses `Random`.
- Browser: wasm threads need `SharedArrayBuffer`, which needs cross-origin isolation headers
  (COOP/COEP). GitHub Pages cannot set headers; a service worker can add them. Measure first.

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
