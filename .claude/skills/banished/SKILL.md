---
name: banished
description: Entry point for any work on the Banished 1:1 Rust/wgpu/WASM reimplementation in this repo. Use first whenever a task touches Banished, the original binaries, game-data, crates/formats, crates/game, apps/game, or docs/ here; it gives the ground rules and routes to the specialised banished-* skills.
---

# Banished reimplementation: ground rules and routing

Goal: a **1:1** reimplementation of Banished (Steam build 1.0.7, Build 170910, Windows x64) in Rust +
wgpu that runs natively and in the browser (WASM), reading the user's own `.pkg` data. "1:1" means
behaviour, data formats, constants and UI match the original executable — not "inspired by".

## Non-negotiable rules

1. **Never commit game material.** `game-data/` (original binaries, `.pkg`, extracted `.crs`, derived
   PNGs/emulator dumps) and `analysis/` (Ghidra project, `decomp.c`, disassembly dumps) are
   git-ignored and must stay that way. Do not paste decompiled C, disassembly listings, shader
   bytecode, or resource dumps into tracked files. Docs describe findings in prose, tables and
   addresses (`FUN_1400be770`), never copied code.
2. **Evidence before code.** Every ported behaviour cites the original function it came from: the
   doc comment is plain English and ends with ``Original: `FUN_1400be680`.``; offsets, vtables and
   `DAT_` constants go in `//` comments (rustdoc does not show them). If a value or rule is a
   guess, say `inferred`/`assumed` in the comment and the doc; a value produced by running the
   original under the emulator is *measured* (name the script and inputs).
3. **Other sessions edit this repo concurrently.** Stage only your own files/hunks; never
   `git add -A`, never revert changes you did not make. See `banished-land`.
4. **Back up as you go.** Commit and push to `origin main` after each coherent piece of work.
5. **Downloads need user permission** (state filename, source, size).

## Map of the repo

| Path | What |
|---|---|
| `crates/formats` | Dependency-free readers: `.pkg` (`pkg::Archive::read_ref("Path/X.rsc:object")`), `.crs` types, UI dialogs, `game.rs` gameplay resources, `menu_scene`, `save` |
| `crates/game` | Bit-exact game logic (Random, Perlin, terrain generator, surface, mesh, queries) |
| `crates/ui` | The game's UI system (element tree, layout, fonts, controls, input, events) |
| `crates/gfx`, `crates/audio`, `crates/wma` | wgpu UI renderer/uploads; xWMA playback; WMA v2 decoder |
| `apps/game` (bin `unbanished`) | The game: `loops.rs` engine loops, `menu.rs`, `gameplay.rs`, `render.rs`, `scene*.rs`, `models.rs`, `world.rs`, `*.wgsl` |
| `apps/viewer` | Asset browser (models, textures, audio, dialogs) |
| `tools/ghidra` | Headless export scripts, `readdata.py` (constants), `vtable.py` |
| `tools/emu` | Unicorn harness `pe_emu.py` + ground-truth generators |
| `tools/crs`, `tools/schema` | Python `.crs` decoders/validator; type-id → class name map |
| `tools/shader/d3d9_disasm.py` | D3D9 shader disassembler for the game's compiled programs |
| `docs/` | Findings. `FINDINGS.md` (overview), `GAME-LOOP-CHECKLIST.md` (live progress), `GAME-LOOP-STEP-NN.md`, topic docs |
| `analysis/export/<binary>/decomp.c` | Full Ghidra pseudocode, `// ==== <addr> <name>` per function (local only) |

Binaries: `Application-steam-x64.exe` (game logic, base `0x140000000`), `Runtime-steam-x64.dll`
(engine, base `0x180000000`, exports mangled C++ names), `VideoDX9/DX11` (renderer backends).

Setup:

```bash
tools/worktree-setup.sh     # in a git worktree (agents usually are): links game-data, save-game,
                            # analysis, .venv and .env from the main checkout
tools/re-setup.sh           # checks Ghidra, JDK 21, objdump, Node, .venv; --install adds what is missing
tools/extract-assets.sh     # in the main checkout, if game-data/ is missing
tools/ghidra/export.sh      # ~20 min, if analysis/export is missing
```

Ghidra and Java are found through `.env` (`GHIDRA_INSTALL_DIR`, `JAVA_HOME`), never hard-coded.
REA (the `mcp__rea__*` tools) starts from the project `.mcp.json`: use it for quick, cited answers
about one function (`open_binary`, `analyze_function`, `procedure_callers`, `xrefs`) and the text
export for broad searches. `./gameframe run rea_session` runs REA from a script with the
project's labels and a saved snapshot (banished-trace, section 2b). The generic method behind these skills, for any game, is
https://github.com/tanepiper/reverse-engineer-games (its `re-*` skills).

## Which skill for which task

| Task | Skill |
|---|---|
| "How does the original do X?" — find and understand a function, constant, vtable, string | `banished-trace` |
| Need exact numbers/bit-exact output from an original function | `banished-emulate` |
| A `.crs`/`.rsc` resource or save block isn't parsed yet | `banished-format` |
| Turn traced behaviour into Rust (logic, loops, UI dialogs, controllers) | `banished-port` |
| Port a material/shader to WGSL | `banished-shader` |
| Prove a change works (tests, headless captures, web build) | `banished-verify` |
| Record a video of the game or a model (headless frames → WebM) | `banished-video` |
| Write up findings, update the checklist, commit and push safely | `banished-land` |

A typical feature runs: **trace → (emulate) → (format) → port → verify → land**. Do not skip
`land`; undocumented findings get re-derived by the next agent.

Before starting, read `docs/GAME-LOOP-CHECKLIST.md` and grep `docs/` for the function addresses or
resource names you are about to work on — much has already been traced by other sessions.
