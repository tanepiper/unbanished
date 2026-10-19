# AGENTS.md — working on Un-Banished

Instructions for anyone changing this repository: human contributors and AI coding agents
(Claude Code, Codex, Cursor and others). Players want the [README](README.md) and the
[user guide](docs/USER-GUIDE.md) instead.

## The goal

Un-Banished is a **1:1 reimplementation** of Banished 1.0.7 (Steam build 170910, Windows x64) in
Rust + wgpu. It runs natively and in the browser (WASM) on the player's own game data. "1:1" means
the behaviour, data formats, constants, timings and interface match the original executable,
down to float rounding where the simulation depends on it. It is not "inspired by".

1:1 applies to what the player can observe, not to how the port is built inside. Where it changes
nothing observable (rules, numbers, Random order, saves, visuals), make the port efficient in
memory, CPU and GPU: share assets instead of copying them per instance, upload each texture
once, avoid rebuilding unchanged data every frame. The browser build runs in a 4 GB wasm heap, so
waste shows up as crashes and stutter. Look for such waste while you work, measure it before and
after, and fix it; `determinism.sh` and the emulator tests must still pass. A change like this is
not a deviation.

The code names keep the project's working name: crates `banished-*`, the binary `banished`, the
settings folder `banished-wasm`. The product name is **Un-Banished**; don't rename code
identifiers as part of unrelated work.

## Non-negotiable rules

1. **Never commit game material.** `game-data/` (original binaries, `.pkg`, extracted `.crs`,
   derived images, emulator dumps), `analysis/` (Ghidra project, `decomp.c`, disassembly) and
   save files (`*.sav`, e.g. in `save-game/`) are git-ignored and stay that way. Never force-add past `.gitignore`. Never paste
   decompiled C, disassembly, shader bytecode or resource dumps into tracked files: describe
   findings in prose, tables, addresses (`FUN_1400be770`), offsets (`+0x44`) and constants.
   The one exception is the handful of screenshots in `docs/assets/screenshots/` that show the
   game running for the README and setup guide: compressed WebP of Un-Banished's own frames,
   made with `tools/screenshots.sh`. Add to them only when the docs need it.
2. **Evidence before code.** Every ported behaviour cites the original function it came from in a
   doc comment (`/// Port of FUN_1400be680: …`). A guess is marked *inferred* or *assumed* in the
   comment and the docs. Check against the decompile or the emulator; don't invent behaviour.
3. **Mark every intentional difference** in a code comment containing "Deviation:" (or
   "not ported"), then regenerate the ledger with `python3 tools/triage/deviations.py` and commit
   `docs/DEVIATIONS.md` with the change.
4. **Other sessions edit this repo at the same time.** Stage only your own files or hunks. Never
   `git add -A`, `git add .` or `git commit -a`, and never revert changes you didn't make.
5. **Back up as you go.** Commit and push to `origin main` after each coherent, verified piece of
   work.
6. **Ask before downloading** anything (say the file, the source and the size). Treat downloaded
   files as untrusted.

## Setup

```bash
tools/extract-assets.sh            # copies your game into game-data/ (or BANISHED_DIR=… tools/extract-assets.sh)
cargo test --release               # tests needing game data skip when it is missing
tools/ghidra/export.sh             # optional, ~20 min: analysis/export/<binary>/decomp.c
```

- Ghidra 12.1.x with JDK 21 (`brew install ghidra` on macOS;
  `GHIDRA_INSTALL_DIR=/opt/homebrew/opt/ghidra/libexec`).
- Python emulator dependencies live in `.venv` (`unicorn`, `pefile`).
- Web builds: `rustup target add wasm32-unknown-unknown`, `cargo install trunk`, then
  `tools/dev-web.sh --app game --release`.

Binaries under study: `Application-steam-x64.exe` (game logic, base `0x140000000`) and
`Runtime-steam-x64.dll` (engine, base `0x180000000`, exports mangled C++ names).

## Map of the repository

| Path | What |
|---|---|
| `apps/game` | The game (binary `banished`): `loops.rs` engine loops, `menu.rs`, `gameplay.rs`, `render.rs`, `start/` (new game, load, save), the UI windows, `*.wgsl` |
| `apps/viewer` | Asset browser (models, textures, audio, dialogs) |
| `apps/cli` | Command-line helpers |
| `crates/game` | Bit-exact game rules: terrain, RNG, citizens, jobs, wildlife, path finding, CRT maths, … |
| `crates/formats` | Dependency-free readers: `.pkg`, `.crs`, UI dialogs, gameplay resources, saves |
| `crates/ui` | The game's UI system (element tree, layout, fonts, controls, input) |
| `crates/gfx`, `crates/audio`, `crates/wma` | wgpu helpers, xWMA playback, WMA v2 decoder |
| `tools/emu` | Unicorn harness `pe_emu.py` and ground-truth generators for the tests |
| `tools/ghidra` | Headless export scripts, `readdata.py` (constants), `vtable.py` |
| `tools/triage` | `deviations.py`, `coverage.py` (uncited functions), `determinism.sh` |
| `tools/github`, `.github/` | `sync-labels.sh` (the label set), issue forms, PR template |
| `site/` | The website (VitePress): `npm --prefix site run build`; pages from `docs/` and the README are generated by `site/scripts/prepare.mjs` |
| `tools/perf` | `samply_top.py` summarises samply profiles; see `docs/PERFORMANCE.md` for `BANISHED_PROFILE` |
| `tools/logo` | `make_logo.py` regenerates the logo, icon and favicon in `docs/assets/` |
| `docs/` | Findings. Start with `FINDINGS.md`, `GAME-LOOP-CHECKLIST.md`, `PARITY-AUDIT.md` |

Before starting, read `docs/GAME-LOOP-CHECKLIST.md` and grep `docs/` for the addresses or
resources you are about to touch: much has already been traced.

## How work flows

A typical change runs: **trace → (emulate) → port → verify → document → land.** Project skills
in `.claude/skills/` spell each step out (agents without skill support can read the `SKILL.md`
files directly):

| Task | Skill |
|---|---|
| Find and understand an original function, constant, vtable or string | `banished-trace` |
| Get exact numbers from an original function (Unicorn emulation) | `banished-emulate` |
| Parse a `.crs`/`.rsc` resource or save block | `banished-format` |
| Turn traced behaviour into Rust | `banished-port` |
| Port a material or shader to WGSL | `banished-shader` |
| Prove a change works (tests, captures, web build) | `banished-verify` |
| Document, commit and push safely | `banished-land` |

### Exactness checklist

The details that drift are the ones that bite:
- Float operation order matters. Port the original's term order, and use `crate::crt`
  (`sinf`, `cosf`, `acosf`, `atan2f`) and `crate::math` (normalise, heading, quaternion
  `Transform`) rather than Rust's `f32` methods in simulation code.
- `System::Time` is integer microseconds; convert as `scheduler::seconds` / `ticks` do.
- Iteration order, the game `Random` call order and CRT `qsort` instability are observable;
  `ai::crt_qsort_by` reproduces the latter.
- Where possible, add an emulator check (`tools/emu/*.py` writing to
  `game-data/derived/emu/`, a test in `crates/game/tests/` that skips without it).

## Versions

The engine version is `0.1.0-<build>`. Do not confuse it with the Banished version: the main menu
footer ("Version 1.0.7 - Build 170910") comes from the original's resources and must not change.

- `0.1.0` is set once, as `version` in `[workspace.package]` in the root `Cargo.toml`. Every
  crate inherits it with `version.workspace = true`.
- `apps/game/build.rs` sets the build id at compile time, for native and wasm32 builds:
  `UNBANISHED_BUILD` from the environment if it is set, else the short git hash (7 characters)
  with `-dirty` for uncommitted changes to tracked files, else `GITHUB_SHA`, else `unknown`.
  `.github/workflows/site.yml` sets `UNBANISHED_BUILD`. The site footer uses the same rule
  (`site/.vitepress/config.mts`).
- It appears in `banished --version`, the first log line, the browser's loading panel and the
  site footer (`apps/game/src/version.rs`). Do not add it to the original's menus unless the
  change is marked as a Deviation.
- Bump the workspace version only for a release, in its own commit: the minor number for new
  features or parity work, the patch number for fixes only. Before 1.0 there are no
  compatibility promises. Ordinary commits do not change it; the build id identifies them.

## Before every commit

```bash
cargo test --offline -q --workspace --release --no-fail-fast
tools/triage/determinism.sh 22 1234
cargo build --offline --release -p banished-app --target wasm32-unknown-unknown   # 0 warnings
rustfmt --edition 2021 <the files you changed>
python3 tools/triage/deviations.py                                                # stage docs/DEVIATIONS.md
```

Captures and tests must not write the player's settings or saves (`BANISHED_SETTINGS`,
`BANISHED_USER_DIR`).

## Committing

- Stage files explicitly: `git add path/one path/two`. For a shared file holding someone else's
  edits, stage only your hunks (see `.claude/skills/banished-land/SKILL.md`).
- Subject in the imperative, at most 72 characters; the body says what was traced or ported and
  how it was verified.
- AI agents end the message with their attribution line, e.g.
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- `git show --stat HEAD` to confirm only your files went in, then
  `git pull --rebase --autostash && git push`. If the rebase conflicts in files you didn't touch,
  stop and report.

## Writing docs

Match the existing docs: an opening paragraph on scope and evidence type; prose and tables keyed
by function address and field offset; every claim observed, marked *inferred*, or listed under
**Open questions**; no copied pseudocode. Tick `docs/GAME-LOOP-CHECKLIST.md` when an item is done.
Player-facing changes (controls, file locations, new options) also go in
[docs/USER-GUIDE.md](docs/USER-GUIDE.md).
