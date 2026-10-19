# 6. Verify a change

A change is verified when you have evidence that you can look at: a test result, or a picture
that you opened and inspected. A successful build is not evidence that a change works.

## Run the tests

```bash
cargo test --release                                        # the whole workspace
cargo test --release -p banished-formats --test toolbar     # one test file
cargo test --release -p banished-game --test terrain_emu    # one parity test
```

Tests that need `game-data/` print a message and pass when it is missing. Read the output and
check that the assertions really ran. [Testing](../TESTING.md) explains the kinds of tests.

## Capture the game without a window

The game can run headless and save a frame as a PNG. Keep the captures in
`game-data/derived/captures/`, which Git ignores. Set `BANISHED_USER_DIR` and
`BANISHED_SETTINGS` to a scratch folder so that captures never change your own saves or
settings.

```bash
# The boot sequence: splash, loading, menu. Capture at 4.5 seconds.
target/release/banished --boot-capture game-data/derived/captures/menu.png 4.5

# Another menu scene (0 to 9)
BANISHED_MENU_SCENE=3 target/release/banished --boot-capture game-data/derived/captures/scene3.png 4.5

# A new game: press named interface elements at given times
target/release/banished --boot-capture game-data/derived/captures/game.png 9 \
  --press buttonNew@4.5 --press buttonOk@5.0

# A saved town, at speed 5, with the Production Limits window open
target/release/banished --boot-capture game-data/derived/captures/town.png 30 \
  --script "load save-game/Save83.sav@4.5" --script "speed 5@12" --script "open Template/UtilityLimits.rsc@20"

# A generated map without starting the game
target/release/banished --capture game-data/derived/captures/map.png --view overview \
  --seed 290 --terrain Valleys --size 0 --month 6
```

`--press NAME@T` clicks the element called `NAME` at `T` seconds. If the element does not exist
at that time, the log says `--press NAME: not found at T s`. Treat that as a failure, not as
noise. The element names come from the dialog resources (use the `dialog_info` example). To test
a toolbar menu, chain presses by element name, with about 0.3 seconds between them, after the
game loop's fade.

The [game's README](../../../apps/game/README.md#headless-captures) lists all the options and
script commands.

To check motion (an animation, citizens walking, the camera, the weather), record a short video
in the same way: one `screenshot` command for each frame. See [Recording videos](../VIDEOS.md).

## Capture the viewer

```bash
target/release/banished-viewer --screenshot game-data/derived/captures/d.png dialogs MainMenu_resource
```

## Build and try the browser version

```bash
cargo build --release -p banished-app --target wasm32-unknown-unknown   # must have 0 warnings
tools/dev-web.sh --app game --release     # http://127.0.0.1:8733/?data=/game-data/
```

Open the page and read the browser console for errors. Sound starts only after the first click
on the page; this is a rule of the browsers.

## Check that the game is still deterministic

```bash
tools/triage/determinism.sh 22 1234
```

It plays the same new game twice with seed 1234 and compares the two frames. Run it after every
change to the simulation, the citizens' decisions, or anything that draws random numbers.

## Compare pictures

```bash
cargo run -q --release -p banished-app --example imgdiff -- before.png after.png --out heat.png
```

`imgdiff` reports the mean error, the PSNR, the share of pixels that differ, where they are,
and a 4 by 4 grid of differences. `--ignore x,y,w,h` masks a region such as the clock.
`--resize` compares pictures of different sizes.

When a change is meant to change the picture, prove that it changes nothing else: see
[Changing what is drawn](../VISUAL-CHANGES.md).

## Compare with the original game

When you have a screenshot of the original game at the same view, compare the layout, the
colours and the text side by side. List the concrete differences; "it looks right" is not a
result. For a stream of the original game (Steam Remote Play), see
`tools/reference/README.md`: it files the reference frames in `game-data/reference/` (ignored by
Git). Compare them with `imgdiff --resize --blur 2`. Write what you observe in the topic
document, under "Observed in the original", with the seed, the settings and the game time.

## Report what you checked

Say exactly what ran and what you looked at: the test names, the number that passed and skipped,
the captures that you opened, and anything that did not match.
