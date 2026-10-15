---
name: banished-verify
description: Verify Banished reimplementation changes — cargo tests (with game-data skip behaviour), headless boot/map captures of the banished app with scripted button presses, viewer screenshots, and the WASM/web build. Use after any port, format or shader change and before committing.
---

# Verify a change

Prefer evidence you can look at: a test result or a PNG you have opened with the Read tool.
Never report "works" from a build alone.

## Tests

```bash
cargo test --release                                   # whole workspace
cargo test --release -p banished-formats --test toolbar
cargo test --release -p banished-game --test terrain_emu
```
Tests that need `game-data/` print a skip message and pass when it is missing — check the output
actually ran the assertions (look for the skip text) before claiming coverage.

## Headless captures of the game (`banished` = `banished-app`)

Captures go to `game-data/derived/captures/` (git-ignored); open them with Read to inspect.

```bash
# Boot flow: splash → loading → menu, capture at T seconds
cargo run --release -p banished-app -- --boot-capture game-data/derived/captures/menu.png 4.5
BANISHED_MENU_SCENE=3 cargo run --release -p banished-app -- --boot-capture …/scene3.png 4.5

# Into a new game: press named UI elements at times (repeatable --press NAME@T)
cargo run --release -p banished-app -- --boot-capture …/game.png 9 \
  --press buttonNew@4.5 --press buttonOk@5.0

# Map only, with generation options
cargo run --release -p banished-app -- --capture …/map.png --view overview|ground \
  --seed 290 --terrain Valleys --size 0 --month 6
```
`--press` warns `not found at T s` if the element name doesn't exist at that moment — treat that as
a failure, not noise. Element names come from the dialog resource (`dialog_info` example). To
test a toolbar/sub-menu, chain presses by element name with ~0.3 s gaps after the game loop's fade.

## Viewer

```bash
cargo run --release -p banished-viewer -- --screenshot game-data/derived/captures/d.png dialogs MainMenu_resource
```

## Web (WASM)

```bash
cargo build --release -p banished-app --target wasm32-unknown-unknown   # compile check
tools/dev-web.sh --app game --release     # serves http://127.0.0.1:8733/?data=/game-data/
```
For an interactive check, open that URL in the built-in browser pane (launch config `game-web`)
and read the console for errors. Audio only starts after the first click (browser gesture rule).
The pane's emulated viewport can size the canvas oddly; real browsers are fine — don't chase it.

## Compare with the original

When a screenshot of the original game exists (user-provided or in
`game-data/derived/screens/`), compare layout, colours and text side by side and list concrete
differences rather than "looks right".

## Report

State exactly what ran and what you looked at: test names and pass/skip counts, capture paths you
opened, and anything that didn't match. Send key captures to the user with SendUserFile when they
are following along.

## Determinism and screenshot comparison

- `tools/triage/determinism.sh [SECS] [SEED]`: two same-seed boot captures must be identical.
  Run it after touching simulation, AI or anything that draws from the game stream.
- `cargo run -q --release -p banished-app --example imgdiff -- A.png B.png [--out heat.png]
  [--resize] [--ignore x,y,w,h] [--json]`: MAE/PSNR, mismatch share, bounding box and a 4×4 grid
  of where frames differ; use it for before/after checks and against original-game screenshots.

## Against the original game (Steam Remote Play)

When the original is streamed to this Mac, capture reference frames and compare: see
`tools/reference/README.md` (file captures with `tools/reference/save.sh` into the git-ignored
`game-data/reference/`; compare with `imgdiff --resize --blur 2`). Write behaviour you observe
into the topic doc under "Observed in the original" with seed, settings and in-game time.
