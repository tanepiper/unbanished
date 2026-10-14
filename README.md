# banished-decompile

Research toward a cross-platform, browser-capable (WASM) reimplementation of the engine for
[Banished](https://www.shiningrocksoftware.com/) by Shining Rock Software.

This repository contains **no game code and no game assets**. It holds format documentation, tooling
and, over time, an engine reimplementation. To use it you need your own copy of the game. The tools read
the original data from your install into local, git-ignored folders.

## Layout

| Path | Contents | In git? |
|---|---|---|
| `docs/` | Reverse-engineering findings and format specs | yes |
| `tools/` | Extractors, decoders, Ghidra scripts | yes |
| `game-data/original/` | Copy of your game's binaries and `.pkg` files | **no** |
| `game-data/extracted/` | Decompressed resources | **no** |
| `game-data/derived/` | Exported PNGs, captures | **no** |
| `analysis/` | Ghidra project and decompiled output | **no** |

## Getting started

Requirements: Python 3.10+ and Rust (stable, with `rustup target add wasm32-unknown-unknown` and
`cargo install trunk` for the web build). For analysis you also need Ghidra 12.1.x and JDK 21
(`brew install ghidra` on macOS).

```bash
# 1. Copy and extract your game data (auto-detects Steam; or set BANISHED_DIR)
tools/extract-assets.sh

# 2. Native asset browser: models, textures, audio, dialogs (reads game-data/original/WinData)
cargo run -p banished-viewer --release

# 3. Web version on http://127.0.0.1:8731/?data=/game-data/ (or pick your .pkg files in the page)
tools/dev-web.sh --release

# The game: boots like the original (splash, loading, main menu with music and 3D town),
# New Game generates the map. Web: tools/dev-web.sh --app game --release (port 8733)
cargo run -p banished-app --release
cargo run -p banished-app --release -- --boot-capture menu.png 4.5
cargo run -p banished-app --release -- --capture map.png --view overview

# Headless screenshot of the browser in a mode, e.g. the main menu dialog
cargo run -p banished-viewer --release -- --screenshot menu.png dialogs MainMenu_resource

# Tests (the game-data tests skip if game-data/ is missing)
cargo test --release

# Optional: regenerate the Ghidra analysis (about 20 minutes)
tools/ghidra/export.sh
```

| Crate | Purpose |
|---|---|
| `crates/formats` | Dependency-free readers for `.pkg`, `.crs`, textures, meshes, skeletons, sounds, UI resources |
| `crates/ui` | Reimplementation of the game's UI system: element tree, layout, fonts, drawing, input |
| `crates/wma` | Pure-Rust WMA v2 decoder for the game's xWMA audio (native and WASM) |
| `crates/game` | The 1:1 game logic: bit-exact `Random`, `PerlinNoise`, terrain generator, surface image and terrain mesh |
| `crates/gfx` | Shared wgpu pieces: UI renderer (incl. the splash Logo material), texture upload |
| `crates/audio` | xWMA playback for native (rodio) and web (Web Audio) |
| `apps/game` | `banished`: the game itself: boot sequence, main menu with music and the 3D menu scenes, New Game dialog, generated maps |
| `apps/viewer` | Asset browser built on `crates/ui`: native window, browser (WASM), `--screenshot`, `--capture` |
| `apps/cli` | `banished-cli export-audio`: decode all sounds to MP3 via FFmpeg |

Viewer keys: **1–4** switch modes; **Up/Down** browse assets. Model previews can play/pause clips,
step to the next clip, and seek with **Left/Right**. The model preview has three exclusive render
modes: “Textured” displays material maps, “Vertices” shows the mesh's indexed vertices as points,
and “Wireframe” draws triangle edges. The sidebar
groups assets by mode, then package folder type, folder category, and matching resources (for
example, Models → Animals → Chicken → its mesh and material resources). **Tab/Shift-Tab** move UI focus,
**Enter/Escape** activate OK/Cancel or close dropdowns, and arrow keys adjust focused sliders/lists.
Edit boxes accept text and cursor keys. Dropdown choices and live game values are supplied by host
controllers; the asset viewer previews templates without a running simulation.

## Status

See [docs/FINDINGS.md](docs/FINDINGS.md) for the format/engine findings and milestone plan, and
[docs/GAMEPLAY.md](docs/GAMEPLAY.md) for the evidence-backed gameplay and data-flow research log.
The first menu-to-new-game trace is in [docs/NEW-GAME-FLOW.md](docs/NEW-GAME-FLOW.md), with the
terrain and starting-condition handoff in [docs/WORLD-GENERATION.md](docs/WORLD-GENERATION.md).

- [x] M1: package (`.pkg`) reader and extractor
- [x] M2: resource (`.crs`) deserializer (core graphics types; others as needed)
- [x] M3: model viewer, native and browser (terrain moves to M5)
- [x] M4: UI system and main menu (all decoded controls render; see FINDINGS for fidelity limits)
- [ ] M5: map generation and terrain (in progress: generator, surface image and terrain mesh are
      bit-exact with the original executable under emulation and match real saves; `banished`
      renders generated maps with the ported terrain shader and monthly lighting; the app boots to the
      main menu with music and the 3D menu scenes. Next: water, sky, decals, particles,
      resource/tree placement, starting town)
- [ ] M6: simulation components
- [ ] M7: save/load, audio in game, polish (audio decoding done)

## Legal

Banished is © Shining Rock Software, LLC. This project is an independent, non-commercial
interoperability effort and is not affiliated with or endorsed by Shining Rock Software. Do not commit
or distribute game files, extracted assets or decompiled code.
