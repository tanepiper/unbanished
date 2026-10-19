# banished (the game)

The game program of Un-Banished. It runs Banished 1.0.7 from the game data of your own copy: the
splash screen, the main menu with its 3D scenes and music, new games, saved games, tutorials,
scenarios, achievements, mods and options, as in the original.

The package is `banished-app`; the program is `banished` (`banished.exe` on Windows). The same
code builds for the desktop and, with Trunk, for the browser.

## Build and play

```bash
cargo build -p banished-app --release
target/release/banished /path/to/Banished/WinData
```

Without a path, the game looks for `WinData` next to the program, then for
`game-data/original/WinData` in the project folder (made by `tools/extract-assets.sh`). The
[setup guide](https://tanepiper.github.io/unbanished/guide/) explains every step for Windows, macOS and Linux, and
[Building](https://tanepiper.github.io/unbanished/develop/building) explains the browser build.

## Command-line options

| Command | What it does |
|---|---|
| `banished [WINDATA]` | Play, with the game data in `WINDATA` |
| `banished --version` (or `-V`) | Print the version, for example `Un-Banished 0.2.0-93be55d`, and stop |
| `banished --boot-capture OUT.png SECONDS [OPTIONS]` | Run the game without a window for `SECONDS`, then save the frame |
| `banished --capture OUT.png [OPTIONS]` | Generate a map and save a picture of it, without starting the game |

## Environment variables

| Variable | Effect |
|---|---|
| `BANISHED_USER_DIR` | The folder that holds `Save` and `Screenshots`. The default is `Documents/Banished`. |
| `BANISHED_SETTINGS` | The settings file. The default is in the system's configuration folder (see [Saves and settings](https://tanepiper.github.io/unbanished/guide/saves)). |
| `BANISHED_SEED` | Fixes the seed of new games and of the menu's own random numbers (for captures and tests) |
| `BANISHED_MENU_SCENE` | Shows this main-menu scene (0 to 9) instead of a random one |
| `BANISHED_PROFILE` | `1` logs slow frames and frame-time summaries; `hitches` logs slow frames only. See [Performance](https://tanepiper.github.io/unbanished/develop/performance). |
| `BANISHED_THREADS` | How many threads loading uses. The default is one for each CPU core; `1` loads on one thread. |
| `RUST_LOG` | How much the log shows, for example `RUST_LOG=debug` |
| `UNBANISHED_BUILD` | At build time: the build id in the version (see [Building](https://tanepiper.github.io/unbanished/develop/building#the-build-id)) |

In the browser, `?data=URL` loads the game data from a folder on a server (for
`tools/dev-web.sh`), and `?profile` turns on the frame profiler.

## Headless captures

Captures run the game without a window, at 30 frames per second of game time, as fast as the
computer can. They play no sound, and they do not read your settings. They do write saves and
screenshots, so set `BANISHED_USER_DIR` and `BANISHED_SETTINGS` to a scratch folder.

### `--boot-capture`

The game starts as normal (splash, loading, main menu), runs for `SECONDS`, and saves the last
frame. These options act at a given time, `@T`, in seconds from the start. Each one can be given
more than once.

| Option | What it does |
|---|---|
| `--press NAME@T` | Click the interface element called `NAME` (the name in the dialog resource) |
| `--click-at X,Y@T` | Move the pointer to `X,Y` (pixels) and click there |
| `--move X,Y@T` | Move the pointer to `X,Y` without clicking |
| `--wheel STEPS@T` | Turn the mouse wheel (zoom the camera) |
| `--combo NAME=ITEM@T` | Choose the item with the text `ITEM` in the combo box `NAME` (New Game settings) |
| `--script "COMMAND@T"` | Run a script command (see below) |
| `--event KIND[:CELL]@T` | Post a game event, for captures of the event ribbon and the log |
| `--capture-size WxH` | The size of the frame. The default is 1280x800. |
| `--ui-scale PERCENT` | The interface scale |
| `--setting KEY=VALUE` | A stored setting by its original name, for example `VideoReflectionDetail=2` |

If `--press` names an element that does not exist at that time, the log says
`--press NAME: not found at T s`.

### Script commands

| Command | Where | What it does |
|---|---|---|
| `load PATH` | main menu | Load the save file at `PATH` |
| `speed N` | game | Set the game speed: 1, 2, 5 or 10 |
| `toggle_pause` | game | Pause or resume, as the Space key does |
| `pause` | game | Open the pause menu |
| `save PATH` | game | Save the game to `PATH` after the next frame |
| `screenshot` / `screenshot ui` | game | Save a screenshot without / with the interface, as Print Screen and Scroll Lock do. It goes to `Screenshots/Screenshot<N>.bmp` in the user folder. |
| `open TEMPLATE` | game | Open or close a utility window, for example `open Template/UtilityMap.rsc` |
| `tool TEMPLATE` | game | Start the tool of a building or a toolbar button |
| `place TEMPLATE` | game | Place a building on the first clear place near the start |
| `remove TEMPLATE RADIUS` | game | Drag a removal tool over the square around the start |
| `rotate` | game | Turn the building being placed by one step |
| `select` | game | Select the building placed last |
| `profession INDEX COUNT` | game | Set the number of workers of a profession |
| `event KIND [CELL]` | game | Post a game event (the same as `--event`) |

### Examples

```bash
# The main menu
banished --boot-capture menu.png 4.5

# A new game with seed 1234, zoomed in
BANISHED_SEED=1234 banished --boot-capture new.png 16 \
  --press buttonNew@4.5 --press buttonOk@5.0 --wheel 8@9

# A saved town at speed 5, with the map window open
banished --boot-capture town.png 30 --capture-size 1600x900 \
  --script "load save-game/Save83.sav@4.5" --script "speed 5@6" \
  --script "open Template/UtilityMap.rsc@25"

# A frame sequence for a video: one screenshot every 1/30 s from 20 to 24 s
args=(); for i in $(seq 0 119); do args+=(--script "screenshot@$(echo "20 + $i / 30" | bc -l)"); done
BANISHED_USER_DIR=/tmp/frames banished --boot-capture last.png 24 \
  --script "load save-game/Save83.sav@4.5" "${args[@]}"
```

### `--capture`

Generates a map and draws it, without the menus or the simulation.

| Option | Values | Default |
|---|---|---|
| `--seed N` | any number | `290` |
| `--terrain NAME` | `Valleys`, `Mountains` | `Valleys` |
| `--size N` | `0` small, `1` medium, `2` large | `0` |
| `--month N` | `0` to `11` | `3` |
| `--view overview` | the whole map from above; without it, a view from the ground | ground |

```bash
banished --capture map.png --view overview --seed 290 --terrain Valleys --size 0 --month 6
```

## Example program: imgdiff

`examples/imgdiff.rs` compares two PNG files: the mean error, the PSNR, the share of pixels that
differ, where they are, and a heat map.

```bash
cargo run -q --release -p banished-app --example imgdiff -- a.png b.png --out diff.png --json
```

Options: `--threshold N`, `--resize`, `--ignore x,y,w,h`, `--crop-a x,y,w,h`, `--autocrop-a`,
`--crop-b x,y,w,h`, `--blur R`, `--compose panels.png`, `--print-crop`.

## How the code is organised

The game follows the original's structure: one engine loop runs at a time, and each original
controller or dialog has its own module.

| Path | What it contains |
|---|---|
| `src/main.rs` | The program: windows, events, the headless captures |
| `src/loops.rs` | The engine loops: `MainLoop`, the loaders, `Screen` with its dialogs |
| `src/menu.rs` | The main menu and its dialogs |
| `src/gameplay.rs`, `src/gameplay/` | The game loop (`GameplayLoop`), split by topic: `simulation`, `input`, `tools`, `hud`, `audio`, `saving`, `tutorial` |
| `src/achievements.rs`, `src/achievements/` | Achievements and scenario goals |
| `src/mods.rs` | Finding and loading BanishedKit mods |
| `src/start.rs`, `src/start/` | Starting a new game (`templates`, `placing`, `citizens`, `animals`, `sites`, `models`, …), and loading (`load/`) and saving (`save/`) a game |
| `src/saves/` | Saved games, the save store and the autosave |
| `src/ui/` | The game's windows: buildings, citizens, the Town Hall, trade, options, mods |
| `src/view/` | The world as drawn: the generated world, the scene, the camera |
| `src/gfx/` | Rendering: the renderer, models, decals, particles, shadows, highlights, screenshots, `terrain.wgsl` |
| `src/audio/` | Music, ambience and sound effects |
| `src/tools/` | The placement, removal and path tools, selection, following a citizen |
| `src/platform/` | Desktop and browser differences, paths, the profiler |
| `src/settings.rs`, `src/keymap.rs` | The settings file and the key bindings |

[Port a behaviour](https://tanepiper.github.io/unbanished/develop/guides/port) explains the engine loops and the dialogs.
