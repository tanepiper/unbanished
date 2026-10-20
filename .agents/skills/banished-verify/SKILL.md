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
