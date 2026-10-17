<p align="center"><img src="assets/un-banished-logo.svg" alt="Un-Banished" width="560"></p>

# Un-Banished user guide

This guide covers installing Un-Banished, pointing it at your game data, and where it keeps your
saves, settings and mods. For what the project is, see the [README](../README.md).

- [What you need](#what-you-need)
- [Build Un-Banished](#build-un-banished)
- [Play on the desktop](#play-on-the-desktop)
- [Play in the browser](#play-in-the-browser)
- [Controls](#controls)
- [Saves](#saves)
- [Settings](#settings)
- [Mods](#mods)
- [Command-line options](#command-line-options)
- [Troubleshooting](#troubleshooting)
- [Reporting a difference from the original](#reporting-a-difference-from-the-original)

## What you need

1. **A copy of Banished** (1.0.7, from Steam or GOG). Un-Banished reads four files from the game's
   `WinData` folder: `data0.pkg`, `data1.pkg`, `patch0.pkg` and `patch1.pkg`. It includes none of
   the game's content itself.

   Where to find `WinData`:

   | Install | Folder |
   |---|---|
   | Steam, Windows | `C:\Program Files (x86)\Steam\steamapps\common\Banished\WinData` |
   | Steam, Linux | `~/.steam/steam/steamapps/common/Banished/WinData` |
   | Steam, macOS | `~/Library/Application Support/Steam/steamapps/common/Banished/WinData` |
   | Other | the game's install folder (the one with its `.exe` files), then `WinData` |

   The game is Windows-only, but Steam on macOS and Linux can still download it: use Steam's
   console (`steam://open/console`) and run `download_depot 242920 242921`, or install it on any
   Windows machine and copy the `WinData` folder across.

2. **A graphics card** that supports Vulkan, Metal, DirectX 12 or OpenGL ES 3 — anything from the
   last ten years.

3. **To build it** (until prebuilt releases are published):
   - [Rust](https://rustup.rs/), stable toolchain;
   - Python 3.10 or newer;
   - for the browser version: `rustup target add wasm32-unknown-unknown` and `cargo install trunk`.

## Build Un-Banished

```bash
git clone https://github.com/tanepiper/banished-decompile.git un-banished
cd un-banished
cargo build -p banished-app --release
```

The game is now at `target/release/banished` (`banished.exe` on Windows).

### Copy in your game data (optional)

When you run from the source folder, Un-Banished looks for the game data in `game-data/`. Fill it
with:

```bash
tools/extract-assets.sh
```

It checks the usual Steam folders. If your copy is somewhere else, tell it where:

```bash
BANISHED_DIR="/path/to/Banished" tools/extract-assets.sh
```

`BANISHED_DIR` is the folder that *contains* `WinData`. The copy goes to `game-data/`, which git
ignores, so it never ends up in a commit.

## Play on the desktop

There are three ways to point Un-Banished at your data, tried in this order:

1. **A folder on the command line:**
   ```bash
   target/release/banished "/path/to/Banished/WinData"
   ```
2. **Next to the game.** Copy `banished` (or `banished.exe`) into your Banished folder, next to
   `WinData`, and run it from there. It then behaves like the original executable: it reads
   `WinData` beside it and uses the original's save folder.
3. **From the source folder**, using the copy made by `tools/extract-assets.sh`:
   ```bash
   cargo run -p banished-app --release
   ```

The game starts as the original does: the splash screen, loading, then the main menu with its
music and 3D town. From there, New Game, Load, Tutorials, Achievements, Mods and Options all work
as in Banished.

## Play in the browser

The browser build is the same game compiled to WebAssembly.

```bash
tools/dev-web.sh --app game --release
```

Open <http://127.0.0.1:8733/>. A panel asks for your game data: select **all four** `.pkg` files
from `WinData` at once (`data0.pkg`, `data1.pkg`, `patch0.pkg`, `patch1.pkg`). The page reads them
locally; nothing is uploaded anywhere.

If you ran `tools/extract-assets.sh`, you can skip the file picker:
<http://127.0.0.1:8733/?data=/game-data/> loads the extracted copy through the local dev server.

**Browser notes**
- Tested in recent Chrome. WebGPU is used where the browser has it, with WebGL 2 as a fallback.
- Click the game once so it receives keyboard input.
- Saves, settings and mods are kept in the browser's own storage for this page (see below).
  Clearing the site's data removes them.
- `tools/dev-web.sh` is a local development server. Don't expose it to the internet.

## Controls

These are the original's defaults. Rebind any of them under **Options → Input**.

### Camera

| Action | Default |
|---|---|
| Move | **W A S D**, or push the pointer against a screen edge (Options → Game → Edge Scroll) |
| Rotate | **Q** / **E**, or hold the **middle mouse button** and move the mouse |
| Zoom | **Mouse wheel**, or **Insert** / **Delete** |
| Tilt | **Page Up** / **Page Down** |
| Camera presets | **Z**, **X**, **C**, **V**, **B** |

### Game

| Action | Default |
|---|---|
| Select a building or citizen, place, confirm | **Left click** |
| Cancel a drag, or put the current tool away | **Right click** / **Esc** |
| Pause and resume | **Space** |
| Toolbar buttons | **F1 – F9** |
| Tools in the open toolbar menu | **1 – 9**, **0** |
| Turn the building being placed | the *Structure Left / Right* keys (see Options → Input) |
| Road tool: straight-line paths | hold the *Path Method* key |
| Screenshot without the interface | **Print Screen** |
| Screenshot with the interface | **Scroll Lock** |

Screenshots go to `Documents/Banished/Screenshots` on the desktop; the browser downloads them.

## Saves

| Version | Where saves live |
|---|---|
| Desktop | `Documents/Banished/Save/` — the same folder as the original game |
| Browser | the browser's storage (IndexedDB) for the page |

On the desktop, Un-Banished uses Banished's own save format and folder, so towns saved by the
original appear in the Load dialog, and autosaves work as in the original (**Options → Game**).
On Windows the folder is `%USERPROFILE%\Documents\Banished\Save` (a Documents folder redirected
elsewhere is not followed).

To keep your saves somewhere else, set `BANISHED_USER_DIR` to a folder; Un-Banished uses its
`Save` subfolder.

## Settings

The Options dialog stores its values (video, audio, game options, key bindings) in a settings file:

| System | File |
|---|---|
| Windows | `%APPDATA%\banished-wasm\settings.txt` |
| macOS | `~/Library/Application Support/banished-wasm/settings.txt` |
| Linux | `$XDG_CONFIG_HOME/banished-wasm/settings.txt` (usually `~/.config/…`) |
| Browser | the browser's local storage for the page |

Delete the file to return to the defaults. `BANISHED_SETTINGS` can name a different file.

## Mods

Un-Banished loads mods made with the BanishedKit (`.pkm` files), as Banished 1.0.7 does.

- **Desktop:** put the `.pkm` files in the `WinData` folder Un-Banished reads, then open **Mods**
  from the main menu, tick the ones you want, set their order and press OK. The game reloads its
  data with the mods applied.
- **Browser:** select the `.pkm` files together with the four `.pkg` files, or drop them onto the
  page at any time, then enable them in the **Mods** menu. They are kept in the browser's storage.

Mods that change the same files are flagged as conflicting, as in the original. As in the
original, achievements are not awarded while mods are enabled.

## Command-line options

To play you need at most one argument: the `WinData` folder to read (`banished PATH`). The rest
render a single picture and exit, for testing and for screenshots in the docs:

| Command | Effect |
|---|---|
| `banished PATH` | play, reading the game data from this `WinData` folder |
| `banished --boot-capture FILE.png SECONDS` | start the game, run it for `SECONDS` and save that frame |
| `banished --capture FILE.png [options]` | generate a map and save a picture of it |

Options for the captures: `--seed N`, `--terrain NAME`, `--size 0|1|2`, `--month 0..11`,
`--view overview`, `--ui-scale PERCENT` and `--setting KEY=VALUE` (a stored setting by its
original name, such as `VideoReflectionDetail=2`). Captures ignore your saved settings.

Environment variables: `BANISHED_USER_DIR` (the folder that holds `Save`), `BANISHED_SETTINGS`
(the settings file), `RUST_LOG` (log detail, e.g. `RUST_LOG=debug`).

## Troubleshooting

**"No .pkg files found."** Un-Banished could not find the game data. Pass the `WinData` folder on
the command line, place the executable next to `WinData`, or run `tools/extract-assets.sh`.

**`tools/extract-assets.sh` says it could not find Banished.** Set `BANISHED_DIR` to the folder
that contains `WinData` and run it again.

**The browser page stays on "Load your game data".** Select all four `.pkg` files in one go. If it
reports a parse error, one of the files is incomplete; verify the game files in Steam and try
again.

**A black window, or a graphics error at start.** Update your graphics drivers. On Linux, make
sure Vulkan drivers are installed (`vulkaninfo` should list your GPU).

**No sound in the browser.** Browsers start audio only after you interact with the page: click
the game once.

**My old Un-Banished settings look wrong after an update.** Delete the settings file listed above.

## Reporting a difference from the original

Un-Banished aims to behave exactly like Banished 1.0.7. If something looks, sounds or plays
differently, please open an issue with:

- what you did, and what the original does instead;
- your system (OS, desktop or browser, GPU);
- if you can, the save file (`Documents/Banished/Save/*.sav`) and a screenshot from both games.

Known, deliberate differences are listed in [DEVIATIONS.md](DEVIATIONS.md), and what has been
verified is in [PARITY-AUDIT.md](PARITY-AUDIT.md).
