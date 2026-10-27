# The launcher: Un-Banished in GameFrame's frame

Un-Banished can start in [GameFrame](https://github.com/tanepiper/game-frame)'s frame, the
launcher that GameFrame's other ports (POD, Hardwar) use. The frame opens first. The player
chooses the game and their copy of Banished's files there, then presses Start, and the frame
starts the game. The original has no launcher, so the whole of it is an extension: the
`unstable-frame` feature and its own program in `apps/frame`. The core (`unbanished-sim`) never
depends on it.

GameFrame describes the contract in its `crates/frame/README.md` and the frame's views in
`crates/frame-ui/README.md`. This page covers what Un-Banished does with them.

## Two programs

| Program | Where | What it does |
|---|---|---|
| `unbanished-frame` | `apps/frame/`, its own Cargo workspace | The frame's window (Dioxus): Play, Settings, a Help page and the event console. It starts `unbanished --frame-io` with GameFrame's `Respawn` and keeps its settings in `settings.json` in its data folder |
| `unbanished --frame-io` | `apps/game/src/frame/`, feature `unstable-frame` | The kernel. It prints its manifest, answers the frame until `game.start`, then plays one game in its own window and ends |

They are two programs because the frame's window and the game's window each need the main
thread on macOS, and winit makes its event loop only once in a process. `Respawn` starts the
game's program again for each start. The launcher has its own workspace so that Dioxus stays out
of the game's build. The root `Cargo.toml` excludes it.

Both depend on GameFrame's crates by git, at one pinned revision
(`05068f7f71d13f2032fdc369e08e791e9173e0a1`). Move both pins together.

## The two machines

| Setting `machine` | What runs | How |
|---|---|---|
| `banished`, "Banished 1.0.7, exactly the original" | The core and the original's own mods, every extension off | The kernel adds `--vanilla` |
| `unbanished`, "Un-Banished, with its extensions" | The core with every extension built into the program | No `--vanilla` |

A program built without `more` offers Banished only. `gameframe install` builds both kinds
(below), so each application offers what it holds.

## The manifest

`apps/game/src/frame/manifest.rs` builds it. Contract `1.1`, capabilities `start`, `renderers`
and `settings`.

- **Renderer.** One, `wgpu` (Metal on macOS, Vulkan or Direct3D 12 elsewhere), graded `good`.
  The original chose between Direct3D 9 and Direct3D 11. The port has one renderer, so the frame
  offers no choice.
- **Settings.**

| Key | Kind | Group | Use |
|---|---|---|---|
| `machine` | choice | Game | The machine, above |
| `data.install` | choice from the game | Game files | `found` (the first copy found), each copy found, or `folder` |
| `data.folder` | text | Game files | For `folder`: the Banished folder, its `WinData`, or any folder with `data0.pkg` |
| `sound.mute` | toggle | Sound | Start silent, as `UNBANISHED_MUTE=1` |
| `game.args` | text | Advanced | More of `unbanished`'s options, split at spaces (`--ui-scale 2`, `--seed 1234`) |

Resolution, full screen and the other video settings stay in the game's own Options window.
They are the original's settings, and the game saves them in its own settings file.

## Events

| Event | The kernel's answer |
|---|---|
| `frame.ready` | Keeps the saved settings and sends `config.options` for `data.install`: `found`, each copy `platform::steam::installs` finds, then `folder` |
| `config.set` | Keeps the new value |
| `game.start` | Makes the launch (`frame::Launch`: the `WinData` folder, vanilla or not, mute, more options) and reports `loading`, or `failed` with the reason: no copy found, no `data0.pkg` in the chosen folder, or a machine this program cannot run |
| `game.stop` | Before the start: `stopped`. While the game runs: `Link::stopping` becomes true, and the window's loop leaves as the game's own quit does |
| (the game's window has a game) | `running`, once |
| (the window closes) | `stopped`; `failed` when the folder holds no packages |

Panics become GameFrame reports (`gameframe_report`), which the frame lists in Play with a
link to file an issue.

## The doors

The kernel reaches the game in three places, each behind `#[cfg(feature = "unstable-frame")]`:

- `main.rs`: `frame::take_args` turns `--frame-io` into the program's usual arguments (the data
  folder, `--vanilla`, the player's options) before anything else reads them, and reports the end.
- `app/handler.rs`, `about_to_wait`: `running` once the game exists, and leaving the loop when
  the frame asks the game to stop.
- `UNBANISHED_MUTE`, set before the sound device opens when the player chose silence.

## Finding the game's files

`apps/game/src/platform/steam.rs` finds Banished. It is part of the game and no extension: it
only finds files, and plain `unbanished` uses it too when it has no other folder
(`platform::paths::data_dir`, after the repository's `game-data/`). Nothing is copied. The game
reads `WinData` in place, with the mods that are in it.

| System | Steam folders searched |
|---|---|
| Windows | `%ProgramFiles(x86)%\Steam`, `%ProgramFiles%\Steam` |
| macOS | `~/Library/Application Support/Steam`; Steam for Windows in every CrossOver bottle (`~/Library/Application Support/CrossOver/Bottles/*/drive_c/Program Files (x86)/Steam`); Wine (`$WINEPREFIX`, else `~/.wine`) |
| Linux | `~/.steam/steam`, `~/.local/share/Steam`, Flatpak's `~/.var/app/com.valvesoftware.Steam/.local/share/Steam`; Wine |

In each Steam folder it reads every library listed in `steamapps/libraryfolders.vdf` and keeps
`steamapps/common/Banished/WinData` where `data0.pkg` is. A library on another drive of a
bottle (`D:\Games`) maps to the bottle's `drive_d`. `UNBANISHED_DIR`, the folder that holds
`WinData`, comes first, as in `tools/extract-assets.sh`.

## Installing

`install.json` at the root is read by GameFrame's `./gameframe install`:

```bash
./gameframe install            # Un-Banished: the Banished machine only (no extensions built in)
./gameframe install extra      # also Un-Banished More: both machines
./gameframe install extra --dry-run
```

| Build | Application | Game program |
|---|---|---|
| vanilla | Un-Banished | `unbanished` built with `--no-default-features --features unstable-frame` |
| extra | Un-Banished More | `unbanished` built with `--features unstable-frame` (the default `more` included) |

On macOS they go to `/Applications/GameFrame/NAME.app`, with their settings in
`~/Library/Application Support/GameFrame/NAME`. On Windows and Linux GameFrame adds a Start menu
or menu entry. Build each system's install on that system.

## Running it by hand

```bash
cargo build --release -p unbanished-app --features unstable-frame
cargo build --release --manifest-path apps/frame/Cargo.toml
apps/frame/target/release/unbanished-frame --unbanished target/release/unbanished --data /tmp/frame
apps/frame/target/release/unbanished-frame --no-frame -- --ui-scale 2
cargo test -p unbanished-app --features unstable-frame frame::
cargo test --manifest-path apps/frame/Cargo.toml
```

## Not yet

- The Binaries release workflow builds `unbanished` only. To ship the launcher it needs the
  frame's build and both programs in each zip.
- The browser build has no frame. The browser's start page is the web page itself.
- No way back to the frame from the running game (`frame.open` from the game's menu). The frame
  comes back when the game's window closes.
- The extended machine's planned additions (renderer upgrades, high-resolution interface
  scaling) will appear here as renderers and settings when they exist.
- The frame's Library tab is off. Saves stay in `Documents/Banished`, where the original keeps
  them.
