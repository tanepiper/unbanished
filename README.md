<p align="center">
  <img src="docs/assets/un-banished-logo.svg" alt="Un-Banished" width="760">
</p>

<p align="center">
  <strong>Play <em>Banished</em> natively on Windows, macOS and Linux — or right in your browser.</strong><br>
  A from-scratch reimplementation in Rust, wgpu and WebAssembly that runs on the game data you already own.
</p>

<p align="center">
  <a href="docs/USER-GUIDE.md">User guide</a> ·
  <a href="#quick-start">Quick start</a> ·
  <a href="#controls">Controls</a> ·
  <a href="#faq">FAQ</a> ·
  <a href="AGENTS.md">Contributing</a> ·
  <a href="docs/FINDINGS.md">Research notes</a>
</p>

---

## What is Un-Banished?

[Banished](https://www.shiningrocksoftware.com/) is a city-building game by Shining Rock Software
about a group of exiled travellers who start a new life in an unforgiving land. Version 1.0.7
shipped as a 32/64-bit Windows program.

**Un-Banished** is a new engine that plays the same game everywhere. It reads the original game's
data files and recreates the game's rules, menus, graphics and sound from careful study of
how the original works, aiming for a **1:1 match**: the same maps from the same seed, the same
citizens, the same timings and the same save files.

- 🖥️ **Native** on Windows, macOS (Apple Silicon and Intel) and Linux.
- 🌐 **In the browser** via WebAssembly — your game files never leave your machine.
- 💾 **Compatible saves**: the original's save format, so your existing towns load as they are.
- 🧩 **Mods**: BanishedKit `.pkm` mods load through the original's Mods menu.
- 🎓 Tutorials, scenarios and achievements, as in the original.

> [!IMPORTANT]
> **Un-Banished contains no game content.** You need your own copy of Banished
> ([Steam](https://store.steampowered.com/app/242920/Banished/) or GOG) — the engine reads the
> `.pkg` files from its `WinData` folder.

## Status

**Beta.** The whole game loop runs: new games and loading, the town and its citizens through the
seasons, every building and tool, trade, disasters, the event log, options, mods, tutorials and
achievements. Map generation and most game rules are verified bit-for-bit against the original.
Work continues on the remaining fine details; see [docs/PARITY-AUDIT.md](docs/PARITY-AUDIT.md) for
what has been checked and [docs/DEVIATIONS.md](docs/DEVIATIONS.md) for the known differences.

If you find something that behaves differently from the original, please open an issue — those
reports are the most useful thing you can send.

## Quick start

There are no prebuilt downloads yet; build from source with the steps below (about five minutes
on a recent machine).

### 1. Install the tools

- [Rust](https://rustup.rs/) (stable).
- Python 3.10 or newer (for the data helper script).
- For the browser version also: `rustup target add wasm32-unknown-unknown` and `cargo install trunk`.

### 2. Get the code and your game data

```bash
git clone https://github.com/tanepiper/banished-decompile.git un-banished
cd un-banished
tools/extract-assets.sh          # finds your Steam install; or: BANISHED_DIR=/path/to/Banished tools/extract-assets.sh
```

The script copies your game's files into `game-data/`, which git ignores.

### 3. Play

**On your desktop:**

```bash
cargo run -p banished-app --release
```

**In your browser:**

```bash
tools/dev-web.sh --app game --release
```

Then open <http://127.0.0.1:8733/>. Pick `data0.pkg`, `data1.pkg`, `patch0.pkg` and `patch1.pkg`
from Banished's `WinData` folder when the page asks; they are read in the page and never uploaded.

**As a drop-in for the original:** copy the built `target/release/banished` (`banished.exe` on
Windows) into your Banished folder next to `WinData`. It then reads the game data there and uses
the same save folder as the original (`Documents/Banished/Save`).

The [user guide](docs/USER-GUIDE.md) covers all of this in detail, plus saves, mods, settings and
troubleshooting.

## Controls

The defaults are the original's, and can be changed under **Options → Input**.

| Action | Keys / mouse |
|---|---|
| Move the camera | **W A S D**, or push the pointer to the screen edge |
| Rotate | **Q** / **E**, or hold the **middle mouse button** and drag |
| Zoom | **Mouse wheel**, or **Insert** / **Delete** |
| Tilt | **Page Up** / **Page Down** |
| Select, place, confirm | **Left click** |
| Cancel a drag or put the tool away | **Right click** or **Esc** |
| Pause / resume | **Space** |
| Toolbar buttons | **F1 – F9** |
| Tools in an open toolbar menu | **1 – 0** |
| Camera presets | **Z X C V B** |
| Screenshot (without / with the interface) | **Print Screen** / **Scroll Lock** |

## FAQ

**Is this legal? Is it a crack?** No. Un-Banished ships no game code, art, sound or text, and it
cannot run without your own copy of the game. It is an independent, non-commercial
interoperability project.

**Can I use my existing saves?** Yes, on the desktop. It uses the original's save folder
(`Documents/Banished/Save`), so your towns appear in the Load dialog. The browser version keeps its
own saves inside the browser.

**Do mods work?** BanishedKit mods (`.pkm`) work: put them in `WinData` (desktop) or select or drop
them onto the page (browser), then enable them in the Mods menu.

**Which browsers work?** It is tested in recent Chrome; Edge should behave the same. WebGPU is
used where available, with WebGL 2 as a fallback, so other modern browsers may work too.

**Does it run on a Steam Deck / Raspberry Pi / …?** Anything with a GPU that supports Vulkan,
Metal, DirectX 12 or OpenGL ES 3 should work. Reports are welcome.

## For developers

The project is a Rust workspace:

| Path | What it is |
|---|---|
| `apps/game` | The game itself (binary `banished`), native and web |
| `apps/viewer` | An asset browser for models, textures, audio and dialogs |
| `apps/cli` | Command-line helpers (e.g. audio export) |
| `crates/game` | Game rules, checked against the original |
| `crates/formats` | Readers for the game's `.pkg`, `.crs`, mesh, texture, sound and save formats |
| `crates/ui`, `crates/gfx`, `crates/audio`, `crates/wma` | Interface, rendering, audio and the WMA decoder |
| `docs/` | Everything learned about how the original works |
| `tools/` | Data extraction, emulator checks, analysis scripts |

```bash
cargo test --release                 # tests that need game data skip when game-data/ is missing
cargo run -p banished-viewer --release
```

Read **[AGENTS.md](AGENTS.md)** before contributing (by hand or with an AI agent): it has the
project rules, the checks to run before a commit, and where to find things.
[docs/FINDINGS.md](docs/FINDINGS.md) is the starting point for the research notes.

## Legal

Banished is © Shining Rock Software, LLC. Un-Banished is an independent, non-commercial
interoperability project. It is **not affiliated with or endorsed by Shining Rock Software**.
"Banished" is used only to say which game this engine plays.

The Un-Banished source code is licensed under MIT or Apache-2.0, at your option. It includes no
game files, extracted assets or decompiled code, and contributions must not add any.
