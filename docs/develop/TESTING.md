# Testing

This page lists the kinds of tests in Un-Banished, how to run them, and the checks to run before
each commit. [Compatibility between versions](../COMPATIBILITY.md) describes the tools that
compare two versions of the game.

## Test suites

`tools/test/run.py` runs the tests in suites, grouped in three tiers. Each suite has a known
outcome on every computer with the same game data: fixed seeds, a fixed window and page size,
throwaway settings, saves and browser profiles, and waits on the game's own state, never on the
clock.

```bash
tools/test/run.py                  # quick: about 1½ minutes; run it before every commit
tools/test/run.py standard         # quick, then determinism and the browser: about 4 minutes
tools/test/run.py long             # everything: before a release, or after network or save work
tools/test/run.py probe browser    # only these suites
tools/test/run.py --list           # every suite and what it checks
```

| Tier | Suite | Checks | Time |
|---|---|---|---|
| quick | `harness` | every Python and shell tool in `tools/` parses; every skill's front matter is valid, AGENTS.md lists every skill, and every repository path a skill mentions exists (`tools/triage/skills_check.py`); with game data and `.venv`, the emulator loads the binaries; with the Ghidra export, `decomp.py` reads it | 1 s |
| quick | `checks` | changed Rust files are formatted, `docs/DEVIATIONS.md` is up to date, doc comments name no addresses, `unbanished-sim` depends on no drawing, window, interface, sound or network crate | 2 s |
| quick | `unit` | every Rust test in the workspace | 75 s |
| quick | `wasm` | the browser build compiles with no warnings | 1–60 s |
| quick | `probe` | the probe scenarios give exactly their baselines | 8 s |
| standard | `determinism` | two new games with one seed draw identical frames | 25 s |
| standard | `browser` | headless Chrome and Firefox: the start page, the manager, Exit and Quit | 2 min |
| standard | `features` | the game builds with no extensions, natively and for the browser, with no warnings ([Extensions](EXTENSIONS.md)) | 15 s–5 min |
| long | `site` | the website builds | 30 s |
| long | `vanilla` | the build with no extensions plays every compatibility scenario exactly as the full build | 4 min |
| long | `replay`, `lockstep`, `network`, `multiplayer`, `server`, `server-simulate` | the `tools/compat` scripts below | minutes each |

A suite that needs something this computer lacks (game data, Chrome, Firefox, trunk) says so
and is skipped, not failed. The summary goes to `target/test-results/summary.json`; a browser
test that fails leaves a screenshot in `target/test-results/<suite>/`.

### Probe scenarios and baselines

The `probe` suite runs the scenarios in `tools/test/scenarios.json` headless. A scenario is a
list of [probe](PROBE.md) commands at fixed times: click New, type a name and a seed, start the
game, then ask for the town, its stock, its citizens and its animals. The answers are compared
with `tools/test/baselines/<scenario>.json`, line by line. A difference prints as a diff, for
example:

```diff
-  "food": 1800,
+  "food": 1795,
```

When a change is meant to change what the game does, record new baselines and review the diff
before you commit them:

```bash
tools/test/run.py probe --update-baselines
git diff tools/test/baselines/
```

An optimisation or refactor must not change any baseline ([Compatibility](../COMPATIBILITY.md)).
To add a scenario, add an entry to `scenarios.json`, run with `--update-baselines`, read the
new file and commit it. Keep each scenario under about 10 seconds.

### Browser tests

`tools/test/browser/*.test.mjs` drive the browser version in headless Chrome (through its
DevTools protocol) and headless Firefox (through WebDriver BiDi). They need Node 22 or later and
nothing else: no npm packages, no geckodriver. The `browser` suite builds the game into
`target/test-web/game` with trunk and serves it on a free local port.

A test finds and presses things with the [probe](PROBE.md), not with pixel positions:

```js
import { gameFiles, launch, run, serveGame } from './lib.mjs';

await run('example', [
  {
    name: 'A new game starts with ten laborers',
    needs: ['chrome', 'gamedata'],
    async run(ctx) {
      const url = await serveGame(ctx);
      const b = await launch(ctx, 'chrome');      // or 'firefox'
      await b.open(`${url}/`);
      await b.startWithFiles(gameFiles());        // choose the four files, wait for the menu
      await b.newGame(1234, 'Testwood');          // fixed seed and name
      const c = await b.probe('citizens');
      if (c.by_profession.laborer !== 10) throw new Error(`laborers: ${c.by_profession.laborer}`);
    },
  },
]);
```

Useful helpers in `lib.mjs`: `press(query)` clicks; `pressAndLeave(query)` clicks a button that
reloads or closes the page (Exit); `waitLoop('Game/Game')`, `waitDialog('Pause.rsc')`,
`waitMenu()` and `waitFor(expression)` wait for state; `expectReload(action)` waits for the page
to reload after the start page's manager changes something; `key('Escape')` sends a key;
`launch(ctx, 'chrome', { standalone: true })` makes the page behave as an installed app.
Each test gets its own browser and profile, which are removed afterwards. Run one file with
`node tools/test/browser/exit.test.mjs`, and only the tests whose names contain a word with
`UB_TEST_ONLY=Firefox node tools/test/browser/exit.test.mjs`.

Keep each test under about 20 seconds. Loading the game files takes about 5 seconds and a new
game about 5 more; check several things in one test rather than loading the game again.

## Kinds of tests

| Kind | Where | What it checks |
|---|---|---|
| Unit tests | `#[cfg(test)]` modules in `crates/*/src` and `apps/*/src` | One function or one rule, with no game data |
| Corpus tests | `crates/formats/tests/` | Every resource of one type in your game data parses, and known values are correct |
| Parity tests | `crates/game/tests/*_emu.rs` and others | The port gives the same bits as the original function, run in the emulator |
| Save tests | `crates/game/tests/saves.rs`, `crates/formats/tests/savegame.rs` | Real save files load, and what the port computes matches what the original saved |
| The core alone | `crates/sim/tests/headless.rs` | A save loads, plays ten seconds and is written again with `unbanished-sim` alone, with no window or GPU, and two runs end in the same state |
| Determinism | `tools/triage/determinism.sh` | Two runs with the same seed draw identical frames |
| Version comparison | `tools/compat/compare.sh` | Two versions produce identical saves and frames |
| Native and browser | `unbanished --sim-digest` and the web build's `?digest=` | The same save runs to the same game state natively and in the browser ([CROSS-PLATFORM.md](../CROSS-PLATFORM.md)) |
| Multiplayer window | `tools/compat/multiplayer.sh` (`--realtime`, `--text`) | Headless games host and join a room through the Multiplayer window, start from the host's save and agree at every check; in `leave`, one of three quits and the other two play on |
| Network | `tools/compat/network.sh` (`unbanished-server`, `--net-test`) | Several game processes play one game through a server and keep identical games; a scrambled one is resynced; a third player who joins the running game has the host's game ([MULTIPLAYER-PLAN.md](../MULTIPLAYER-PLAN.md)) |
| Server | `tools/compat/server.sh` (`unbanished-server` with kept rooms, `--net-test`) | Three players: one is scrambled and resynced, one leaves, a new one joins; the server keeps the town, restarts, and the host goes on from the kept town with another player; every digest agrees. With `--simulate` the server runs the game itself: two players agree with it at every check, and the scrambled player goes on from the server's save |
| Lockstep | `tools/compat/lockstep.sh` (`--lockstep-test`) | Players in one process, joined by a simulated network, keep identical games ([MULTIPLAYER-PLAN.md](../MULTIPLAYER-PLAN.md)) |
| Replays | `tools/compat/replay.sh` (`--record`, `--replay`) | A recorded game replays from its save to the same game state: the player commands carry every change ([MULTIPLAYER-PLAN.md](../MULTIPLAYER-PLAN.md)) |

## Tests that need game data

Many tests need your copy of the game data in `game-data/`, or emulator output in
`game-data/derived/emu/`. These files are not in the repository. When they are missing, the test
prints a message that says why it skipped, and passes. So a passing test run on a computer
without game data proves less than it seems. Read the output and look for the skip messages.

To get the game data, run `tools/extract-assets.sh` (see [Building](BUILDING.md)). To get the
emulator output for a parity test, run the generator that the test names in its first comment,
for example:

```bash
.venv/bin/python tools/emu/terrain_stages.py game-data/derived/emu/valleys-small-290 290
cargo test --release -p unbanished-game --test terrain_emu
```

## Running the tests

```bash
cargo test --release                                          # the whole workspace
cargo test --release -p unbanished-formats --test toolbar       # one test file
cargo test --release -p unbanished-game random                  # tests whose name contains "random"
```

Use `--release`. Some tests simulate many game days or read every resource, and they are slow in
a debug build.

## Replays, lockstep and multiplayer

These scripts take a binary or a git revision (built clean with `tools/compat/build.sh`). Each
exits with 0 when every game agrees, and keeps what it made in `target/compat/` when one does not.

```bash
tools/compat/replay.sh HEAD          # record games, replay them, compare the digests
tools/compat/lockstep.sh HEAD        # players in one process, over a simulated network
tools/compat/network.sh HEAD         # several game processes through a relay on this computer
tools/compat/multiplayer.sh HEAD     # the Multiplayer window: host, join by code, play, leave
```

To record and replay a game yourself:

```bash
unbanished --boot-capture /tmp/last.png 40 --script "load save-game/Save83.sav@4.5" \
  --record /tmp/game.rec --record-every 60
unbanished --replay save-game/Save83.sav /tmp/game.rec      # exit 1 if the game differs
```

`--record` works with the windowed game too: it records the next game you load from a save. The
[game's README](../../apps/game/README.md) describes `--record`, `--replay`, `--lockstep-test`,
`--net-test` and `--sim-digest` in full. [CROSS-PLATFORM.md](../CROSS-PLATFORM.md) compares the
native and browser builds, and [MULTIPLAYER-PLAN.md](../MULTIPLAYER-PLAN.md) explains why these
tests exist.

## Before every commit

[AGENTS.md](../../AGENTS.md) has the same list.

```bash
rustfmt --edition 2021 <the files you changed>
tools/test/run.py                                      # quick: checks, harness, unit, wasm, probe
tools/test/run.py determinism browser                  # if the change touches the game or the browser page
tools/compat/compare.sh HEAD <your build or commit>    # if the change can touch the simulation
```

The quick tier runs `cargo test`, the browser build, `deviations.py` (stage
`docs/DEVIATIONS.md` if it changed) and `doc_comments.py check` for you. The commands it runs
are below, if you want one on its own.

- `determinism.sh SECONDS SEED` starts a new game twice with the same seed, zooms the camera
  and compares the two frames. Any difference means that something is not deterministic: an
  unseeded random number, the iteration order of a hash map, or uninitialised state.
- `deviations.py` collects every `Deviation:` and `not ported` comment in the code into
  [DEVIATIONS.md](../DEVIATIONS.md).
- `compare.sh` is described in [Compatibility between versions](../COMPATIBILITY.md).

## Headless captures

The game can run without a window, for a fixed number of seconds, and save the last frame as a
PNG. This is how changes to the interface, the rendering and the simulation are checked by eye.
Headless captures do not read your settings, and they play no sound. To keep them away from your
own saves, set `UNBANISHED_USER_DIR` and `UNBANISHED_SETTINGS` to a scratch folder.

```bash
# The main menu, 4.5 seconds after start
target/release/unbanished --boot-capture menu.png 4.5

# A new game: press the New button, then OK, and capture at 16 seconds
target/release/unbanished --boot-capture game.png 16 --press buttonNew@4.5 --press buttonOk@5.0

# A generated map seen from above, without starting the game
target/release/unbanished --capture map.png --view overview --seed 290 --terrain Valleys --size 0
```

The [game's README](../../apps/game/README.md#headless-captures) lists every option. If a
`--press` names an element that does not exist at that time, the log says
`--press NAME: not found at T s`. Treat that as a failure.

## Comparing two pictures

The `imgdiff` example compares two PNG files. It reports the mean error, the share of pixels that
differ, where they are, and it can write a heat map.

```bash
cargo run -q --release -p unbanished-app --example imgdiff -- a.png b.png --out diff.png
```

Useful options: `--threshold N` (the smallest difference that counts), `--ignore x,y,w,h` (mask
a region, for example the clock), `--resize` (compare pictures of different sizes), `--blur R`
(tolerate video compression in a capture of the original game) and `--json`.

## Comparing two saves

Two examples in `unbanished-formats` compare saves:

```bash
cargo run -q --release -p unbanished-formats --example savegame_same -- a.sav b.sav
cargo run -q --release -p unbanished-formats --example savegame_fields_diff -- a.sav b.sav
```

`savegame_same` says whether two saves hold the same game state; it ignores the date in the
header. `savegame_fields_diff` lists the objects and fields that differ.

## Random-draw traces

When a port uses the game's random numbers, every draw must come in the same order as in the
original. `tools/triage/rngdiff.py` compares a trace of the original's draws (recorded in the
emulator) with a trace of the port's draws, and reports the first draw that differs, with the
original function and the Rust file and line. See the
[emulation guide](guides/EMULATE.md#random-draw-parity).
