---
name: banished-verify
description: Verify Banished reimplementation changes — cargo tests (with game-data skip behaviour), headless boot/map captures of the banished app with scripted button presses, viewer screenshots, and the WASM/web build. Use after any port, format or shader change and before committing.
---

# Verify a change

Prefer evidence you can look at: a test result or a PNG you have opened with the Read tool.
Never report "works" from a build alone.

## The test runner (start here)

```bash
tools/test/run.py                     # quick: format, ledger, cargo test, wasm, probe baselines (~1½ min)
tools/test/run.py determinism browser # the game's frames, and the browser page in Chrome and Firefox (~3 min)
tools/test/run.py long                # everything; before a release
tools/test/run.py --list
```
Each suite has a known outcome; one that cannot run here says why and is skipped. Use the quick
tier for every change, add `determinism browser` when the change touches the game, its UI or
the web page, and `long` only now and then. `docs/develop/TESTING.md` has the details.

To check the game's state in a headless run, use the probe instead of reading pixels:
`--probe "ui ref:Pause.rsc kind:button@14" --probe "town@16" --probe-out answers.jsonl`.
In a browser test, `b.probe('town')`, `b.press('text:Exit')`, `b.waitLoop('Game/Game')`
(`tools/test/browser/lib.mjs`). See `docs/develop/PROBE.md`. When a change is meant to alter
the game, run `tools/test/run.py probe --update-baselines` and review `git diff
tools/test/baselines/` before committing.

## Tests

```bash
cargo test --release                                   # whole workspace
cargo test --release -p unbanished-formats --test toolbar
cargo test --release -p unbanished-game --test terrain_emu
```
Tests that need `game-data/` print a skip message and pass when it is missing — check the output
actually ran the assertions (look for the skip text) before claiming coverage.

## Headless captures of the game (`unbanished` = `unbanished-app`)

Captures go to `game-data/derived/captures/` (git-ignored); open them with Read to inspect.

```bash
# Boot flow: splash → loading → menu, capture at T seconds
cargo run --release -p unbanished-app -- --boot-capture game-data/derived/captures/menu.png 4.5
UNBANISHED_MENU_SCENE=3 cargo run --release -p unbanished-app -- --boot-capture …/scene3.png 4.5

# Into a new game: press named UI elements at times (repeatable --press NAME@T)
cargo run --release -p unbanished-app -- --boot-capture …/game.png 9 \
  --press buttonNew@4.5 --press buttonOk@5.0

# Map only, with generation options
cargo run --release -p unbanished-app -- --capture …/map.png --view overview|ground \
  --seed 290 --terrain Valleys --size 0 --month 6
```
`--press` warns `not found at T s` if the element name doesn't exist at that moment — treat that as
a failure, not noise. Element names come from the dialog resource (`dialog_info` example). To
test a toolbar/sub-menu, chain presses by element name with ~0.3 s gaps after the game loop's fade.

## Viewer

```bash
cargo run --release -p unbanished-viewer -- --screenshot game-data/derived/captures/d.png dialogs MainMenu_resource
```

## Web (WASM)

```bash
cargo build --release -p unbanished-app --target wasm32-unknown-unknown   # compile check
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
- `cargo run -q --release -p unbanished-app --example imgdiff -- A.png B.png [--out heat.png]
  [--resize] [--ignore x,y,w,h] [--json]`: MAE/PSNR, mismatch share, bounding box and a 4×4 grid
  of where frames differ; use it for before/after checks and against original-game screenshots.

## A change meant to change the picture

Follow `docs/develop/VISUAL-CHANGES.md`. In short:
- Predict first which objects change and when (e.g. `mesh_info` for mesh parts and distances).
- `cp target/release/unbanished` to a scratch path and compare that copy; later builds overwrite it.
- `tools/compat/compare.sh HEAD <copy>` (run it alone, in the background; several minutes). Game
  state must be identical everywhere; `base/base` must be identical; "NOT COMPATIBLE" from frames
  alone is expected.
- `imgdiff … --out heat.png --json` for each differing frame and for base-1 vs base-2; open each
  heat map with Read and check every red area against the prediction.
- Crop the same region from before, after and heat with
  `magick IN -crop WxH+X+Y +repage -filter point -resize 300% OUT`, put them side by side with
  `+append`, look, and describe the change concretely.
- Frames come from the player's data: keep them out of the repository (share them with
  SendUserFile or a private artifact).

## Working in a git worktree

A worktree has no `game-data/`, `save-game/` or `analysis/`. Symlink them from the main checkout
and add `/game-data`, `/save-game`, `/analysis` to `$(git rev-parse --git-common-dir)/info/exclude`
(a symlink is not matched by the `/game-data/` rule). See `docs/develop/BUILDING.md`.

## Against the original game (Steam Remote Play)

When the original is streamed to this Mac, capture reference frames and compare: see
`tools/reference/README.md` (file captures with `tools/reference/save.sh` into the git-ignored
`game-data/reference/`; compare with `imgdiff --resize --blur 2`). Write behaviour you observe
into the topic doc under "Observed in the original" with seed, settings and in-game time.
