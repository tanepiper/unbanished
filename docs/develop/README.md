# Developer documentation

These pages are for people who want to build Un-Banished from source, change it, or study how
it works. If you only want to play, read the [setup guide](../USER-GUIDE.md) instead.

## What the project is

Un-Banished is a reimplementation of the game Banished 1.0.7 (Steam build 170910, Windows x64).
It is written in Rust. It draws with [wgpu](https://wgpu.rs), so it runs natively on Windows,
macOS and Linux, and in web browsers through WebAssembly.

The goal is the same behaviour as the original program: the same maps, rules, numbers, timings,
save files and interface. The source code of Banished is not available. Every behaviour was
worked out from the original program files, and every ported function names the original
function it came from.

## Where to start

| If you want to | Read |
|---|---|
| Build the game, the viewer and the browser version | [Building](BUILDING.md) |
| Run the tests and the checks before a commit | [Testing](TESTING.md) |
| Check that a change does not change the game's results | [Compatibility between versions](../COMPATIBILITY.md) |
| Measure speed and memory | [Performance](../PERFORMANCE.md) |
| Record a video of the game without a window | [Recording videos](VIDEOS.md) |
| Know the project rules | [Contributing (AGENTS.md)](../../AGENTS.md) |
| Port a behaviour of the original game | [Development guides](guides/README.md) |
| Learn how a part of the game works | [Topics](../topics/README.md) |
| Use one of the libraries | [Apps and crates](CRATES.md) and the [API documentation](API.md) |

## The rules in short

The full rules are in [AGENTS.md](../../AGENTS.md). These are the most important ones:

1. **Never commit game material.** The game files, extracted resources, decompiled code and
   save files stay on your computer. Git ignores the folders that hold them (`game-data/`,
   `analysis/`, `save-game/`).
2. **Evidence before code.** Each ported item cites the original function it came from, in the
   last line of its doc comment: ``Original: `FUN_1400be680` ``. A guess is marked *inferred*
   or *assumed*. See [the doc-comment style](API.md#the-doc-comment-style).
3. **Mark every intentional difference** with a comment that contains `Deviation:`. The list
   of differences, [DEVIATIONS.md](../DEVIATIONS.md), is generated from these comments.
4. **Do not change the game's results** in an optimisation or a refactor. Saves, frames and
   random numbers must stay identical.

## The workspace

| Path | What it contains |
|---|---|
| `apps/game` | The game (binary `banished`) |
| `apps/viewer` | The asset browser (binary `banished-viewer`) |
| `apps/cli` | Command-line tools (binary `banished-cli`) |
| `crates/core` | Exact maths, the game's random numbers, noise, game time |
| `crates/terrain` | Map generation, the terrain mesh, ground queries |
| `crates/path` | The pathfinder and the sector graph |
| `crates/game` | The game rules |
| `crates/formats` | Readers for the game's files |
| `crates/ui` | The game's interface system |
| `crates/gfx`, `crates/audio`, `crates/wma` | Shared rendering, sound playback, the WMA decoder |
| `tools/` | Data extraction, emulator checks, analysis and comparison scripts |
| `docs/` | Research notes about the original game, and these pages |
| `site/` | This website |

[Apps and crates](CRATES.md) describes each one and links to its README.
