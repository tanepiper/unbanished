# The same simulation on every platform

This note asks whether Un-Banished's simulation gives the same results natively and in the
browser, from the same save and the same inputs. It matters for anything two computers must
agree on: a replay, a shared bug report, or a multiplayer game in lockstep (where every player
runs the whole simulation and only the players' commands travel). The evidence is a digest of
the game state, compared between a native build and the browser build on one machine.

## The check

`gameplay::digest` (`apps/game/src/gameplay/digest.rs`) loads a save without a window, sets
speed 10, switches off the autosave, and runs updates of 1/60 s (each one ten simulation
iterations, as in the game). Before the first update and after every `EVERY` updates it encodes
the whole game state that a save would write (`GameplayLoop::save_state`, then
`GameState::encode`) and takes its CRC-32. It prints the update, the year, the time in the year
in ticks, the living citizens and the CRC.

The same code runs on both platforms:

```bash
# Native
UNBANISHED_USER_DIR=/tmp/digest UNBANISHED_SETTINGS=/tmp/digest/settings.txt \
  unbanished --sim-digest save-game/Save83.sav 1200 60

# Browser (tools/dev-web.sh --app game --release; the save is fetched from the dev server)
http://127.0.0.1:8733/?data=/game-data/&digest=/game-data/derived/digest/Save83.sav&updates=1200&every=60
```

The browser build shows the digests in a `<pre id="digest">` at the end of the page and in the
console. `?digest=` works only on a page served from this machine, like `?data=`.

## Results

MacBook Pro M3 (arm64), macOS. Native: the release build for `aarch64-apple-darwin`. Browser:
the release `wasm32-unknown-unknown` build, served by trunk, in the Chromium-based browser pane of
the Claude desktop app.

| Save | Updates (simulation iterations) | Game time | Digests | Native and browser |
|---|---|---|---|---|
| Save83 (116 citizens, year 22) | 1,200 (12,000) | 200 s | 21 | **identical** (two citizens die in both) |
| Save 47 (104 citizens, year 16, a trader boat on its way) | 1,200 (12,000) | 200 s | 21 | **identical** |
| Save83 | 21,600 (216,000) | 3,600 s, a whole year into year 23 | 37 | **identical** (116 citizens fall to 106 in both) |

Two native runs of the same save also give identical digests, so the digest itself is
repeatable.

The digest covers everything a save keeps: citizens and their action stacks, buildings and
work, stock, wildlife, trader boats, the calendar, the weather and the game's `Random` state.
Runtime-only state (the pathfinder's caches, the sector graph) is not hashed itself, but any
difference in it that changes the game shows up in a later digest.

## Why they agree

None of this was added for the check; it follows from how the port is built.

- **One fixed step.** The game updates in fixed 1/60 s steps from an accumulator
  (`FRAME_STEP`); a game speed is that many simulation iterations per step. Nothing in the
  simulation reads the frame rate or the wall clock. Game time is integer microseconds
  (`scheduler`).
- **One thread.** The simulation, the pathfinder included, runs on one thread on every platform.
- **Exact maths.** Simulation code uses the port's own `crt` (`sinf`, `cosf`, `acosf`,
  `atan2f`) and `math` functions, which give the original's bits, instead of the platform's
  maths library. Rust does not fuse multiply and add on its own. WebAssembly's `f32` and `f64`
  operations are IEEE 754 with the same rounding as native code.
- **Ordered data.** Entities are indices, and the simulation's maps are ordered (`BTreeMap`), so
  iteration order never depends on memory addresses or hash seeds.
- **Its own random numbers.** The simulation draws from the game's `Random`, whose state is in
  the save. Sounds and particles draw from their own generators.

## What this means for multiplayer

The original game needed a hooked DLL and a separate deterministic heap, synchronous pathfinding
and per-step updates before two Windows computers agreed (see the Banished Multiplayer mod's
design notes). In the port, the native build and the browser build already agree, over a whole
game year. A lockstep design could therefore mix native and browser
players, as long as these stay true and the remaining local random draws are separated:
starting the placement tool, the Path tool's pick and one menu click draw from the simulation's
`Random` on the local computer only (1:1 with the original), and would have to become commands
or use a generator of their own. [MULTIPLAYER-PLAN.md](MULTIPLAYER-PLAN.md) describes how, without
changing the single-player game.

## Open questions

- **x86_64.** Only arm64 native was compared. The same reasoning applies to x86_64 (SSE2 `f32`
  arithmetic is IEEE, and Rust does not use the x87 unit there), but it has not been run.
- **Other browsers.** Only the Chromium-based browser pane was used. Firefox and Safari run the
  same WebAssembly rules; untested.
- **New games.** The check starts from saves. Generating a new map (terrain, rivers, start
  place) from a seed in the browser has not been compared with native.
- **NaN payloads.** WebAssembly may give a NaN a different bit pattern than native code. No NaN
  was seen in these runs; a NaN that reaches a save could make the digests differ without the
  game behaving differently.
