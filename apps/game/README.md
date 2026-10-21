# banished (the game)

The game program of Un-Banished. It runs Banished 1.0.7 from the game data of your own copy: the
splash screen, the main menu with its 3D scenes and music, new games, saved games, tutorials,
scenarios, achievements, mods and options, as in the original. It also has what the original
does not: a Browse tab in the Mods menu for each mod site (Nexus Mods, and banishedinfo.com on the
desktop) and Multiplayer on the main menu (see the [user guide](https://tanepiper.github.io/unbanished/guide/mods)).

The package is `unbanished-app`; the program is `unbanished` (`unbanished.exe` on Windows). The same
code builds for the desktop and, with Trunk, for the browser.

## Build and play

```bash
cargo build -p unbanished-app --release
target/release/unbanished /path/to/Banished/WinData
```

Without a path, the game looks for `WinData` next to the program, then for
`game-data/original/WinData` in the project folder (made by `tools/extract-assets.sh`). The
[setup guide](https://tanepiper.github.io/unbanished/guide/) explains every step for Windows, macOS and Linux, and
[Building](https://tanepiper.github.io/unbanished/develop/building) explains the browser build.

## Command-line options

| Command | What it does |
|---|---|
| `unbanished [WINDATA]` | Play, with the game data in `WINDATA` |
| `unbanished --vanilla [WINDATA]` | Play vanilla: the original game and its own mods, with every extension off ([docs/develop/EXTENSIONS.md](../../docs/develop/EXTENSIONS.md)) |
| `unbanished --version` (or `-V`) | Print the version, for example `Un-Banished More 0.5.0-93be55d`, and stop |
| `unbanished --boot-capture OUT.png SECONDS [OPTIONS]` | Run the game without a window for `SECONDS`, then save the frame |
| `unbanished --capture OUT.png [OPTIONS]` | Generate a map and save a picture of it, without starting the game |
| `--record FILE [--record-every N]` | With any of these, or with `unbanished [WINDATA]`: record the next game loaded from a save ([Recording and replaying](#recording-and-replaying)) |
| `unbanished --replay SAVE FILE [UPDATES] [EVERY]` | Replay a recording from its save without a window, and check the game state |
| `unbanished --sim-digest SAVE [UPDATES] [EVERY]` | Run a save without a window and print digests of the game state (see `--sim-digest` below) |
| `unbanished --lockstep-test SAVE RECORDING …` | Players in one process over a simulated network ([Lockstep test](#lockstep-test)) |
| `unbanished --net-test RELAY host\|join ROOM SAVE RECORDING PLAYERS …` | One player of a game through a relay, for `tools/compat/network.sh` |

## Environment variables

| Variable | Effect |
|---|---|
| `UNBANISHED_USER_DIR` | The folder that holds `Save` and `Screenshots`. The default is `Documents/Banished`. |
| `UNBANISHED_SETTINGS` | The settings file. The default is in the system's configuration folder (see [Saves and settings](https://tanepiper.github.io/unbanished/guide/saves)). |
| `UNBANISHED_SEED` | Fixes the seed of new games and of the menu's own random numbers (for captures and tests) |
| `UNBANISHED_MENU_SCENE` | Shows this main-menu scene (0 to 9) instead of a random one |
| `UNBANISHED_PROFILE` | `1` logs slow frames and frame-time summaries; `hitches` logs slow frames only. See [Performance](https://tanepiper.github.io/unbanished/develop/performance). |
| `UNBANISHED_THREADS` | How many threads loading uses. The default is one for each CPU core; `1` loads on one thread. |
| `RUST_LOG` | How much the log shows, for example `RUST_LOG=debug` |
| `UNBANISHED_NEXUS_API_KEY` | Your Nexus Mods API key, for installs from the Browse tab. It wins over `NexusApiKey` in the settings file. |
| `UNBANISHED_ROOM` | Tests: the room code the Multiplayer window asks the relay for |
| `UNBANISHED_NET_CHECK` | Tests: how often (in updates) the multiplayer host compares the players' games |
| `UNBANISHED_BUILD` | At build time: the build id in the version (see [Building](https://tanepiper.github.io/unbanished/develop/building#the-build-id)) |

In the browser, `?data=URL` loads the game data from a folder on a server (for
`tools/dev-web.sh`), `?profile` turns on the frame profiler, and `?digest=SAVE_URL` (with
`&updates=N&every=N`) runs a save and prints its digests, like `--sim-digest`. `?data=` and
`?digest=` work only on a page served from this computer.

## Headless captures

Captures run the game without a window, at 30 frames per second of game time, as fast as the
computer can. They play no sound, and they do not read your settings. They do write saves and
screenshots, so set `UNBANISHED_USER_DIR` and `UNBANISHED_SETTINGS` to a scratch folder.

### `--boot-capture`

The game starts as normal (splash, loading, main menu), runs for `SECONDS`, and saves the last
frame. These options act at a given time, `@T`, in seconds from the start. Each one can be given
more than once.

| Option | What it does |
|---|---|
| `--press NAME@T` | Click the interface element called `NAME` (the name in the dialog resource) |
| `--click-at X,Y@T` | Move the pointer to `X,Y` (pixels) and click there |
| `--move X,Y@T` | Move the pointer to `X,Y` without clicking |
| `--mouse-down X,Y@T` / `--mouse-up X,Y@T` | Move the pointer there and press or release the left button: a drag, with `--move` between |
| `--wheel STEPS@T` | Turn the mouse wheel (zoom the camera) |
| `--hold ACTION@START-END` | Hold the default key of a key-map action from `START` to `END` seconds, for example `RotateLeft`, `RotateRight`, `Forward`, `Back`, `Left`, `Right`, `ZoomIn`, `ZoomOut`, `PitchUp` or `PitchDown`: camera moves |
| `--combo NAME=ITEM@T` | Choose the item with the text `ITEM` in the combo box `NAME` (New Game settings) |
| `--script "COMMAND@T"` | Run a script command (see below) |
| `--event KIND[:CELL]@T` | Post a game event, for captures of the event ribbon and the log |
| `--capture-size WxH` | The size of the frame. The default is 1280x800. |
| `--ui-scale PERCENT` | The interface scale |
| `--setting KEY=VALUE` | A stored setting by its original name, for example `VideoReflectionDetail=2` |
| `--text NAME=TEXT@T` | Click the edit box called `NAME`, empty it and type `TEXT` (a room code, a town name) |
| `--realtime` | Keep the frames to the clock instead of running as fast as possible (a capture that talks to a relay) |
| `--scene-scale PART` | Draw the 3D world at that part of the frame's size, for example `0.5`, and scale it up; the interface stays sharp. It tests the browser's Resolution setting natively |
| `--camera …`, `--camera-follow …` | The cinematic camera (see below) |
| `--probe "COMMAND@T"` | Run a [probe](../../docs/develop/PROBE.md) command: find elements (`ui text:New`), click them (`click ref:Pause.rsc text:Exit`), or read the town (`town`, `citizens`, `stock`). The answer is logged as JSON |
| `--probe-out FILE` | Also write every probe answer to `FILE`, one JSON object per line |

If `--press` names an element that does not exist at that time, the log says
`--press NAME: not found at T s`.

### The cinematic camera

For videos, a capture can draw the town from a camera of its own instead of the game's. It
changes only the picture: the game camera, the simulation and saves are as without it. Culling,
shadows, the sky and the weather follow the cinematic camera. Positions are in map cells from the
middle of the map, with `z` up; heading 0 faces `+y`.

| Option | What it does |
|---|---|
| `--camera EX,EY,EZ>TX,TY,TZ@T` | A keyframe: at `T` seconds the camera is at `EX,EY,EZ` and looks at `TX,TY,TZ`. Give several: the camera moves on a smooth curve through them, starts and stops gently, and stays at the last one. Before the first keyframe the game camera draws. |
| `--camera-follow SUBJECT@START[-END]` | Ride with a subject from `START` (to `END`) seconds: `citizen N`, `profession NAME N` (`fish`, `hunt`), `boat N` (a trader boat) or `animal [NAME] N` (`deer`). The camera waits for the subject to appear (a boat that has not arrived yet) and stays where it was when the subject leaves the map. A follow comes before the keyframes. |
| `--camera-rig BACK,UP[,SIDE[,AHEAD[,LOOK_UP]]]` | Where a following camera sits, in the subject's own frame: `BACK` behind it, `UP` above its feet, `SIDE` to its right, looking at the point `AHEAD` in front of it and `LOOK_UP` high. The default `4,2.6,0.8,8,1` is over a citizen's right shoulder. |
| `--camera-fov DEGREES` | The vertical field of view. The game's is 40. |
| `--camera-smooth SECONDS` | How slowly a following camera turns with its subject (default 0.6; it moves with the subject four times faster). |
| `--camera-clear 0` | Let a following camera go through things. By default it looks at the buildings, trees and bridges around the subject (their picking meshes), and when one would come between them it moves in closer, or rises up to 8. |

Run with `RUST_LOG=info,unbanished::view::cinematic=debug` to log the followed subject's position
and heading at each step. It helps to place keyframes: a trader boat's positions trace the
river.

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
| `select citizen N` | game | Select the N-th living citizen, so that `--press buttonFollow` follows them |
| `select profession NAME N` | game | Select the N-th living citizen whose profession's name contains `NAME`, for example `fish` or `hunt` |
| `profession INDEX COUNT` | game | Set the number of workers of a profession |
| `event KIND [CELL]` | game | Post a game event (the same as `--event`) |

### Examples

```bash
# The main menu
unbanished --boot-capture menu.png 4.5

# A new game with seed 1234, zoomed in
UNBANISHED_SEED=1234 unbanished --boot-capture new.png 16 \
  --press buttonNew@4.5 --press buttonOk@5.0 --wheel 8@9

# A saved town at speed 5, with the map window open
unbanished --boot-capture town.png 30 --capture-size 1600x900 \
  --script "load save-game/Save83.sav@4.5" --script "speed 5@6" \
  --script "open Template/UtilityMap.rsc@25"

# A frame sequence for a video: one screenshot every 1/30 s from 20 to 24 s
args=(); for i in $(seq 0 119); do args+=(--script "screenshot@$(echo "20 + $i / 30" | bc -l)"); done
UNBANISHED_USER_DIR=/tmp/frames unbanished --boot-capture last.png 24 \
  --script "load save-game/Save83.sav@4.5" "${args[@]}"
```

### Recording and replaying

`--record FILE` (with any run, a capture or the windowed game) records the next game loaded from a
save: every player command with the update it was given in, as JSON lines. `--record-every N`
adds a digest of the game state every N updates. `unbanished --replay SAVE FILE [UPDATES] [EVERY]`
loads the save without a window, carries the commands out at the same updates, and compares the
digests: it prints `identical` or the first that differs, and exits with 1 when one does.
`tools/compat/replay.sh BINARY` records and replays the compatibility scenarios.

```bash
unbanished --boot-capture /tmp/f.png 40 --script "load save-game/Save83.sav@4.5" ... \
  --record /tmp/game.rec --record-every 60
unbanished --replay save-game/Save83.sav /tmp/game.rec
```

### Lockstep test

`unbanished --lockstep-test SAVE RECORDING [PLAYERS] [DELAY] [JITTER] [STALL] [SEED]` plays a
recording's commands, split between PLAYERS (default 3), in lockstep games joined by a simulated
network: an input delay of DELAY updates (default 6), turns 1 to 1 + JITTER ticks late (default
8), each computer too busy to update in STALL % of ticks (default 20), the network's random seed
SEED. Then CHECK, how often the host compares the players' digests (default 600 updates, 0 for
never), and CORRUPT, `PLAYER:UPDATE`, which scrambles one player's game there: the host must find
it and resync everybody from its save. It prints the desyncs and resyncs, and `identical` when
every player's game agrees at the end; it exits with 1 when not.
`tools/compat/lockstep.sh BINARY` runs it with six settings on the `actions` scenario.

### Network test

`unbanished --net-test RELAY host|join ROOM SAVE RECORDING PLAYERS [DELAY] [CHECK] [CORRUPT]` is one
player of a game played through a relay (`unbanished-server`): the host opens ROOM, waits for
PLAYERS and starts the game from SAVE; the others join ROOM and get the save from the host. Each
plays its share of RECORDING's commands and prints a digest every 60 updates; CORRUPT `UPDATE`
scrambles this player's game there. `tools/compat/network.sh BINARY` runs a relay and three players
and compares them. In the browser build, a local page with
`?data=/game-data/&net=ws://127.0.0.1:PORT&room=ROOM&updates=N` joins as a player (giving no
commands) and shows its digests at the end of the page.

### Multiplayer window

The Multiplayer button on the main menu opens `ui/multiplayer.rs`. A game started there holds its
session (`gameplay/network.rs`, `NetState`): each update reads what arrived, carries out the slot,
sends its turn and, at a check, the host compares digests. A resync fades to the loading screen
and on to the host's save. The players line is the original's version footer without its logo.
`tools/compat/multiplayer.sh BINARY` plays it from end to end with two headless games kept to the
clock (`--realtime`); `--text NAME=TEXT@SECONDS` types into an edit, and `UNBANISHED_ROOM` asks the
relay for a fixed room code.

### `--sim-digest`

`unbanished --sim-digest SAVE [UPDATES] [EVERY]` loads a save without a window, runs it at speed
10 for `UPDATES` updates (default 1200, 200 s of game time) and prints a digest of the game state
every `EVERY` updates (default 60): the time, the living citizens and the CRC-32 of everything
the save would hold. The browser build does the same with
`?data=/game-data/&digest=SAVE_URL&updates=N&every=M` on a local page. Equal digests mean the
two simulations agree; see [docs/CROSS-PLATFORM.md](../../docs/CROSS-PLATFORM.md).

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
unbanished --capture map.png --view overview --seed 290 --terrain Valleys --size 0 --month 6
```

## Example program: imgdiff

`examples/imgdiff.rs` compares two PNG files: the mean error, the PSNR, the share of pixels that
differ, where they are, and a heat map.

```bash
cargo run -q --release -p unbanished-app --example imgdiff -- a.png b.png --out diff.png --json
```

Options: `--threshold N`, `--resize`, `--ignore x,y,w,h`, `--crop-a x,y,w,h`, `--autocrop-a`,
`--crop-b x,y,w,h`, `--blur R`, `--compose panels.png`, `--print-crop`.

## How the code is organised

The game follows the original's structure: one engine loop runs at a time, and each original
controller or dialog has its own module.

| Path | What it contains |
|---|---|
| `src/main.rs` | The program's start: the modules, the shared constants and `main` for the desktop and the browser |
| `src/game.rs` | `Game`: the resources, the loop chain, and the input and drawing every platform shares |
| `src/app.rs` | The windowed game: the winit application, its window, renderer and events |
| `src/native.rs` | The commands without a window: `--boot-capture` (with `--probe`), `--capture`, `--sim-digest`, `--replay`, the lockstep and network tests |
| `src/probe.rs` | The [probe](../../docs/develop/PROBE.md): find and press interface elements and read the town, headless and in the browser (`window.unbanished`) |
| `src/loops.rs` | The engine loops: `MainLoop`, the loaders, `Screen` with its dialogs |
| `src/menu.rs` | The main menu and its dialogs |
| `src/gameplay.rs`, `src/gameplay/` | The game loop (`GameplayLoop`), split by topic: `loading` (behind the loading screen), `frame` (one frame), `raw_input`, `input`, `script` (capture commands), `simulation`, `tools`, `hud`, `audio`, `saving`, `tutorial`, `probe` (world queries) |
| `src/achievements.rs`, `src/achievements/` | Achievements and scenario goals |
| `src/mods.rs` | Finding and loading BanishedKit mods |
| `src/start.rs`, `src/start/` | Starting a new game (`templates`, `placing`, `citizens`, `animals`, `sites`, `models`, …), and loading (`load/`) and saving (`save/`) a game |
| `src/saves/` | Saved games, the save store and the autosave |
| `src/ui/` | The game's windows: buildings (`building_ui/`, by kind of building), citizens, the Town Hall, trade, options, mods |
| `src/view/` | The world as drawn: the generated world, the scene, the camera |
| `src/gfx/` | Rendering: the renderer (`render/`: frames, uploads, render targets, lighting, the minimap, captures), models, decals, particles, shadows, highlights, screenshots, `terrain.wgsl` |
| `src/audio/` | Music, ambience and sound effects |
| `src/tools/` | The placement, removal and path tools, selection, following a citizen |
| `src/platform/` | Desktop and browser differences, paths, the profiler; `web.rs` is the browser page's glue (file picker, kept files, save import, Exit) |
| `index.html`, `web/`, `pwa/` | The browser page: its markup, the start page's style (`web/start.css`), the manager (`web/manager.js`), the console helper (`web/probe.js`), and the app manifest and service worker |
| `src/settings.rs`, `src/keymap.rs` | The settings file and the key bindings |

[Port a behaviour](https://tanepiper.github.io/unbanished/develop/guides/port) explains the engine loops and the dialogs.
