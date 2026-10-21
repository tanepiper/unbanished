# Building

This page explains how to build each part of Un-Banished from source: the game, the viewer, the
command-line tools, the mod manager, the multiplayer relay, the browser versions and this website. The [setup guide](../USER-GUIDE.md)
explains how to install Rust and Git on Windows, macOS and Linux.

## What you need

| Tool | Needed for | Install |
|---|---|---|
| Rust (stable) | everything | <https://rustup.rs> |
| Git | getting the code, the build id | your system's package manager |
| ALSA headers (Linux only) | sound | `sudo apt install libasound2-dev pkg-config` |
| The `wasm32-unknown-unknown` target | the browser versions | `rustup target add wasm32-unknown-unknown` |
| [Trunk](https://trunkrs.dev) | the browser versions | `cargo install trunk` |
| Python 3 | the scripts in `tools/` | your system's package manager |
| Node.js 22 | this website | <https://nodejs.org> |
| A copy of Banished 1.0.7 | running anything, and most tests | Steam or GOG |

## Copy your game data into the project

Most tools and tests read the game data from `game-data/` in the project folder. Git ignores
this folder. Copy your data into it once:

```bash
tools/extract-assets.sh                              # looks in the usual Steam folders
UNBANISHED_DIR="/path/to/Banished" tools/extract-assets.sh
```

`UNBANISHED_DIR` is the folder that contains `WinData`. The script copies the four `.pkg` files to
`game-data/original/WinData/`. For the research tools, it also does these things:

- it copies the original program files (`.exe`, `.dll`) to `game-data/original/`;
- it extracts every resource to `game-data/extracted/`;
- it writes the checksums of the copied files to `game-data/MANIFEST.sha256`;
- if FFmpeg is installed, it decodes every sound to MP3 in `game-data/derived/audio/`.

### In a second checkout (git worktree)

A `git worktree` (AI coding agents often work in one) has none of the ignored folders. Link
them to the main checkout instead of copying them:

```bash
tools/worktree-setup.sh
```

It links `game-data`, `save-game`, `analysis`, `.venv` and `.env` (each one the main checkout
has) and adds the links to the repository's `info/exclude`: a link is a file to Git, so the
`/game-data/` rule in `.gitignore` does not match it. `info/exclude` is shared by every worktree
of the repository and is never committed. Running it again changes nothing.

## The game

```bash
cargo build -p unbanished-app --release
target/release/unbanished                      # reads game-data/original/WinData
target/release/unbanished /path/to/WinData     # or any other WinData folder
```

The package is called `unbanished-app`; the program is called `unbanished`. Always build with
`--release` to play: a debug build is too slow. The workspace builds its dependencies with
optimisation even in debug builds (`[profile.dev.package."*"]` in `Cargo.toml`), so a debug build
of the game is usable for stepping through code.

The [game's README](../../apps/game/README.md) lists every command-line option.

## The viewer, the command-line tools, the mod manager and the relay

```bash
cargo run -p unbanished-viewer --release               # browse models, textures, sounds, dialogs
cargo run -p unbanished-cli --release -- export-audio  # decode every sound to MP3 (needs FFmpeg)
cargo run -p unbanished-cli --release -- nexus search terrain   # mods from Nexus Mods
cargo run -p unbanished-modmanager --release -- path/to/WinData # the mod manager, unbanished-mods
cargo run -p unbanished-server --release                # the multiplayer relay, on 0.0.0.0:7780
```

See the READMEs of the [viewer](../../apps/viewer/README.md), the
[command-line tools](../../apps/cli/README.md), the [mod manager](../../apps/modmanager/README.md)
and the [relay](../../apps/server/README.md). The mod manager's **Play** starts `unbanished` from
beside itself (or from the `PATH`), so build both into the same `target/release` folder.

## The browser versions

The game and the viewer also build for WebAssembly. Trunk builds the program, writes the HTML
page next to it and serves both.

```bash
rustup target add wasm32-unknown-unknown    # once
cargo install trunk                         # once

tools/dev-web.sh --app game --release       # the game:   http://127.0.0.1:8733/?data=/game-data/
tools/dev-web.sh --release                  # the viewer: http://127.0.0.1:8731/?data=/game-data/
```

`tools/dev-web.sh` also serves your `game-data/` folder on port 8732, so `?data=/game-data/`
loads your game data without the file chooser. Without `?data=`, the page asks you to choose
the four `.pkg` files. The script uses its own target folder, `target/web`, so it does not
block other Cargo builds. Do not make this server available on the internet: it serves your
game files.

To check that the browser version compiles without starting a server:

```bash
cargo build --release -p unbanished-app --target wasm32-unknown-unknown
```

This build must finish with no warnings.

## The build id

The version of a build looks like `0.2.0-93be55d`. The first part is the workspace version, from
`[workspace.package]` in `Cargo.toml`. The second part is the build id. `apps/game/build.rs`
chooses it, in this order:

1. the `UNBANISHED_BUILD` environment variable, if it is set;
2. the short git commit hash (7 characters), with `-dirty` when tracked files have changes;
3. the `GITHUB_SHA` environment variable;
4. `unknown`.

`unbanished --version` prints it.

## This website

The website is in `site/`. It uses [VitePress](https://vitepress.dev). Most pages are made from
files in the repository by `site/scripts/prepare.mjs`, so the site and the repository say the same
thing. Edit the file in the repository, not the generated page in `site/`.

```bash
npm --prefix site ci                # once
npm --prefix site run dev           # a local preview with live reload
npm --prefix site run build         # the full build; it fails on a dead link
```

Two parts of the site are built separately, because they take longer:

```bash
site/scripts/build-game.sh          # the browser game, for the Play page (site/public/game/)
site/scripts/build-api.sh           # the Rust API documentation (site/public/api/)
```

The GitHub workflow `.github/workflows/site.yml` runs all three steps and publishes the site
to GitHub Pages after each push to `main`.

## Separate target folders

Several builds can run at the same time if each one uses its own target folder. The project uses
these:

| Folder | Used by |
|---|---|
| `target/` | normal builds |
| `target/web` | `tools/dev-web.sh` |
| `target/compat` | `tools/compat/build.sh` (builds of other revisions) |
| `target/profiling` | profiling builds with line tables (see [Performance](../PERFORMANCE.md)) |
| `target/api` | `site/scripts/build-api.sh` |
