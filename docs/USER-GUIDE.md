<p align="center"><img src="assets/un-banished-logo.svg" alt="Un-Banished" width="560"></p>

# Un-Banished setup guide

This guide takes you from nothing to playing in about ten minutes. It also covers where
Un-Banished keeps your saves, settings and mods, and what to do when something goes wrong. For
what the project is, see the [README](../README.md).

**Setup**

1. [Check what you need](#1-check-what-you-need)
2. [Install the build tools](#2-install-the-build-tools)
3. [Download and build Un-Banished](#3-download-and-build-un-banished)
4. [Find your game data](#4-find-your-game-data)
5. [Play](#5-play)

**Reference**

- [Play in the browser](#play-in-the-browser)
- [Controls](#controls) · [Saves](#saves) · [Settings](#settings) · [Mods](#mods)
- [Command-line options](#command-line-options)
- [Troubleshooting](#troubleshooting)
- [Reporting a difference from the original](#reporting-a-difference-from-the-original)

---

## 1. Check what you need

| You need | Why |
|---|---|
| **Banished 1.0.7** from Steam or GOG | Un-Banished reads the game's art, sound and data from your copy. It includes none of its own. |
| **A graphics card from the last ten years** | Anything with Vulkan, Metal, DirectX 12 or OpenGL ES 3. |
| **About 3 GB of free disk space** | For the build tools and the compiled game. |

Banished itself only runs on Windows, but you only need its data files, and they work on any
system. On a Mac or Linux PC you can get them in either of these ways:

- **Steam console:** open `steam://open/console` in your browser (Steam opens its console), then
  run `download_depot 242920 242921`. Steam prints the folder it downloaded to.
- **Copy from a Windows PC:** install Banished there and copy its `WinData` folder across.

## 2. Install the build tools

Un-Banished is written in [Rust](https://www.rust-lang.org/). There are no prebuilt downloads
yet, so you install Rust once and it builds the game for you. Pick your system:

<details>
<summary><strong>Windows</strong></summary>

1. Download and run **rustup-init.exe** from <https://rustup.rs>.
2. If it asks to install the *Visual Studio C++ build tools*, say yes and accept the defaults.
   Rust needs them to build Windows programs.
3. When it asks how to install Rust, press **Enter** to choose the default.
4. Install **Git** from <https://git-scm.com/download/win> (the defaults are fine).
5. Open a new **PowerShell** or **Terminal** window so it picks up the new tools.

</details>

<details>
<summary><strong>macOS</strong></summary>

1. Open **Terminal** (in Applications → Utilities).
2. Install Apple's command-line tools, which include Git:
   ```bash
   xcode-select --install
   ```
3. Install Rust and accept the default option:
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```
4. Close Terminal and open it again so it picks up the new tools.

</details>

<details>
<summary><strong>Linux</strong></summary>

1. Install a C compiler, Git and the ALSA sound headers. On Debian or Ubuntu:
   ```bash
   sudo apt install build-essential git pkg-config libasound2-dev
   ```
   On Fedora: `sudo dnf install gcc git pkgconf-pkg-config alsa-lib-devel`.
   On Arch: `sudo pacman -S base-devel git alsa-lib`.
2. Install Rust and accept the default option:
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```
3. Open a new terminal so it picks up the new tools. Check that your graphics driver supports
   Vulkan: `vulkaninfo --summary` should list your GPU.

</details>

To check it worked, run `cargo --version` in the new terminal. It should print a version number.

## 3. Download and build Un-Banished

In the terminal, run:

```bash
git clone https://github.com/tanepiper/unbanished.git
cd unbanished
cargo build -p banished-app --release
```

The first build downloads its libraries and takes a few minutes. Later builds are much faster.
When it finishes, the game is at:

| System | File |
|---|---|
| Windows | `target\release\banished.exe` |
| macOS, Linux | `target/release/banished` |

To update later, run `git pull` in the same folder, then the `cargo build` line again.

## 4. Find your game data

Un-Banished reads four files from your game's `WinData` folder: `data0.pkg`, `data1.pkg`,
`patch0.pkg` and `patch1.pkg`. It only reads them and never changes them.

| Where you got Banished | The `WinData` folder |
|---|---|
| Steam, Windows | `C:\Program Files (x86)\Steam\steamapps\common\Banished\WinData` |
| Steam, macOS | `~/Library/Application Support/Steam/steamapps/common/Banished/WinData` |
| Steam, Linux | `~/.steam/steam/steamapps/common/Banished/WinData` |
| GOG or elsewhere | the folder with Banished's `.exe` files, then `WinData` inside it |

> [!TIP]
> In Steam you can right-click **Banished → Manage → Browse local files** to open the game's
> folder.

## 5. Play

Choose whichever of these suits you.

**A. Point it at the folder.** This works anywhere:

```bash
# Windows (PowerShell)
target\release\banished.exe "C:\Program Files (x86)\Steam\steamapps\common\Banished\WinData"

# macOS
target/release/banished ~/Library/Application\ Support/Steam/steamapps/common/Banished/WinData

# Linux
target/release/banished ~/.steam/steam/steamapps/common/Banished/WinData
```

**B. Put it next to the game.** Copy `banished.exe` (or `banished`) into your Banished folder,
next to `WinData`. Run it from there, or make a desktop shortcut to it. It then behaves like the
original program: it finds `WinData` beside it and shares the original's save folder.

**C. Run it from the source folder.** This suits contributors. Copy your data in once, then play
with `cargo run`:

```bash
tools/extract-assets.sh          # finds the usual Steam folders
cargo run -p banished-app --release
```

If your copy is somewhere else, run `BANISHED_DIR="/path/to/Banished" tools/extract-assets.sh`.
`BANISHED_DIR` is the folder that *contains* `WinData`. The script needs Python 3 and a `bash`
shell (on Windows, use Git Bash). The copy goes into `game-data/`, which git ignores, so it never
ends up in a commit.

The game then starts like the original: the splash screen, the loading screen, then the main
menu with its music and a 3D town scene. New Game, Load, Tutorials, Achievements, Mods and Options all
work.

<p align="center">
  <img src="assets/screenshots/main-menu.webp" alt="The Un-Banished main menu" width="720">
</p>

Choose **New**, accept the defaults and press **OK**, and your settlers arrive by their first
stockpile:

<p align="center">
  <img src="assets/screenshots/new-game.webp" alt="The first minute of a new game" width="720">
</p>

Click any building or citizen to open its window. This is the Town Hall in a grown town:

<p align="center">
  <img src="assets/screenshots/town-hall.webp" alt="The Town Hall window in a grown town" width="720">
</p>

---

## Play in the browser

The browser version is the same game compiled to WebAssembly. Your game files are read inside
the page and are never uploaded. It needs two extra tools, installed once:

```bash
rustup target add wasm32-unknown-unknown
cargo install trunk
```

Then start the local server from the source folder:

```bash
tools/dev-web.sh --app game --release
```

1. Open <http://127.0.0.1:8733/> in Chrome or Edge.
2. A **Load your game data** panel appears at the top left. Press **Choose files** and select
   **all four** `.pkg` files from `WinData` together (`data0.pkg`, `data1.pkg`, `patch0.pkg`,
   `patch1.pkg`). Hold Ctrl (Cmd on a Mac) to select several files.
3. Click the game once so it receives keyboard input and can start its sound.

If you ran `tools/extract-assets.sh`, you can skip the file picker by opening
<http://127.0.0.1:8733/?data=/game-data/>, which loads your extracted copy.

**Good to know**
- It uses WebGPU where the browser has it, and WebGL 2 otherwise.
- Saves, settings and mods are kept in the browser's storage for this page (see below).
  Clearing the site's data removes them.
- `tools/dev-web.sh` is a server for your own machine only. Don't make it reachable from the
  internet.

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
differently, or doesn't work, please [open an issue](https://github.com/tanepiper/unbanished/issues/new/choose).
The forms there ask for what's needed:

- **Parity difference:** what the original does, what Un-Banished does, and how to see it.
- **Bug report:** something broken or impossible to do.
- **Performance problem:** stutters, slowdowns, long loads.

Screenshots or a short video from both games help a lot. Please don't attach the game's own
files (`.pkg`, `.exe`, extracted art or sound).

Known, deliberate differences are listed in [DEVIATIONS.md](DEVIATIONS.md), and what has been
verified is in [PARITY-AUDIT.md](PARITY-AUDIT.md).
