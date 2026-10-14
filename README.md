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
| `analysis/` | Ghidra project and decompiled output | **no** |

## Getting started

Requirements: Python 3.10+. For analysis you also need Ghidra 12.1.x and JDK 21
(`brew install ghidra` on macOS).

```bash
# 1. Copy and extract your game data (auto-detects Steam; or set BANISHED_DIR)
tools/extract-assets.sh

# 2. Optional: regenerate the Ghidra analysis (about 20 minutes)
tools/ghidra/export.sh
```

## Status

See [docs/FINDINGS.md](docs/FINDINGS.md) for what is known and the milestone plan.

- [x] M1: package (`.pkg`) reader and extractor
- [ ] M2: resource (`.crs`) deserializer
- [ ] M3: browser model and terrain viewer
- [ ] M4: UI system and main menu
- [ ] M5: map generation and terrain
- [ ] M6: simulation components
- [ ] M7: save/load, audio, polish

## Legal

Banished is © Shining Rock Software, LLC. This project is an independent, non-commercial
interoperability effort and is not affiliated with or endorsed by Shining Rock Software. Do not commit
or distribute game files, extracted assets or decompiled code.
