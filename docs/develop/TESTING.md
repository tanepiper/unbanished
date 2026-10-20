# Testing

This page lists the kinds of tests in Un-Banished, how to run them, and the checks to run before
each commit. [Compatibility between versions](../COMPATIBILITY.md) describes the tools that
compare two versions of the game.

## Kinds of tests

| Kind | Where | What it checks |
|---|---|---|
| Unit tests | `#[cfg(test)]` modules in `crates/*/src` and `apps/*/src` | One function or one rule, with no game data |
| Corpus tests | `crates/formats/tests/` | Every resource of one type in your game data parses, and known values are correct |
| Parity tests | `crates/game/tests/*_emu.rs` and others | The port gives the same bits as the original function, run in the emulator |
| Save tests | `crates/game/tests/saves.rs`, `crates/formats/tests/savegame.rs` | Real save files load, and what the port computes matches what the original saved |
| Determinism | `tools/triage/determinism.sh` | Two runs with the same seed draw identical frames |
| Version comparison | `tools/compat/compare.sh` | Two versions produce identical saves and frames |
| Native and browser | `banished --sim-digest` and the web build's `?digest=` | The same save runs to the same game state natively and in the browser ([CROSS-PLATFORM.md](../CROSS-PLATFORM.md)) |

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
cargo test --release -p banished-game --test terrain_emu
```

## Running the tests

```bash
cargo test --release                                          # the whole workspace
cargo test --release -p banished-formats --test toolbar       # one test file
cargo test --release -p banished-game random                  # tests whose name contains "random"
```

Use `--release`. Some tests simulate many game days or read every resource, and they are slow in
a debug build.

## Before every commit

Run these checks. [AGENTS.md](../../AGENTS.md) has the same list.

```bash
cargo test --offline -q --workspace --release --no-fail-fast
tools/triage/determinism.sh 22 1234
cargo build --offline --release -p banished-app --target wasm32-unknown-unknown   # 0 warnings
rustfmt --edition 2021 <the files you changed>
python3 tools/triage/deviations.py                     # then stage docs/DEVIATIONS.md
python3 tools/triage/doc_comments.py check            # no addresses in the rendered docs
tools/compat/compare.sh HEAD <your build or commit>    # if the change can touch the simulation
```

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
own saves, set `BANISHED_USER_DIR` and `BANISHED_SETTINGS` to a scratch folder.

```bash
# The main menu, 4.5 seconds after start
target/release/banished --boot-capture menu.png 4.5

# A new game: press the New button, then OK, and capture at 16 seconds
target/release/banished --boot-capture game.png 16 --press buttonNew@4.5 --press buttonOk@5.0

# A generated map seen from above, without starting the game
target/release/banished --capture map.png --view overview --seed 290 --terrain Valleys --size 0
```

The [game's README](../../apps/game/README.md#headless-captures) lists every option. If a
`--press` names an element that does not exist at that time, the log says
`--press NAME: not found at T s`. Treat that as a failure.

## Comparing two pictures

The `imgdiff` example compares two PNG files. It reports the mean error, the share of pixels that
differ, where they are, and it can write a heat map.

```bash
cargo run -q --release -p banished-app --example imgdiff -- a.png b.png --out diff.png
```

Useful options: `--threshold N` (the smallest difference that counts), `--ignore x,y,w,h` (mask
a region, for example the clock), `--resize` (compare pictures of different sizes), `--blur R`
(tolerate video compression in a capture of the original game) and `--json`.

## Comparing two saves

Two examples in `banished-formats` compare saves:

```bash
cargo run -q --release -p banished-formats --example savegame_same -- a.sav b.sav
cargo run -q --release -p banished-formats --example savegame_fields_diff -- a.sav b.sav
```

`savegame_same` says whether two saves hold the same game state; it ignores the date in the
header. `savegame_fields_diff` lists the objects and fields that differ.

## Random-draw traces

When a port uses the game's random numbers, every draw must come in the same order as in the
original. `tools/triage/rngdiff.py` compares a trace of the original's draws (recorded in the
emulator) with a trace of the port's draws, and reports the first draw that differs, with the
original function and the Rust file and line. See the
[emulation guide](guides/EMULATE.md#random-draw-parity).
