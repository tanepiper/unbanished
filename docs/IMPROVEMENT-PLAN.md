# Improvement plan: after GameFrame

This plan has two parts. The first part lists what bringing Un-Banished up to GameFrame's standard
added, and what that standard still asks for. The second part lists ways to improve the code
itself, with measurements taken on 2026-10-22. Each step names its gate. None of the steps may
change the game's results: `tools/compat/compare.sh` must stay identical, unless a step says
otherwise.

## Part 1: GameFrame

### What the update added

The update (commits `d255331` and `cbfb6cb`, report in
[develop/GAMEFRAME.md](develop/GAMEFRAME.md)) filled these gaps:

| Gap | What was missing | What changed |
|---|---|---|
| Labels from the port | The port cited about 2,000 original functions, but only 226 had labels. Decompiles still showed `FUN_…` for most of them | `cite_labels.py` writes `labels/names-from-port.tsv` from the citations. 703 functions now have labels, 28.2 % of the Application's code by size (10.2 % before) |
| Type ids | The CRC-32 scheme was known, but no tool checked it against the data | `hashnames.py` names 169 of the 170 types from the files alone |
| Shader source | Not recorded | 31 materials carry GLSL 3.3 source beside the Direct3D bytecode. The shader guide now uses it as a second reference |
| Skeletons | The reader was checked only against our own tests | `skeleton.py` finds the citizen's 27 bones, which confirms the reader |
| Gates | The rules were in AGENTS.md only | GameFrame's plugin refuses commits and files with game material or decompiled code, and refuses `git add -A` and force-pushes |
| Layout | Labels were in `tools/triage/` | `labels/`, `docs/CHECKLIST.md`, `tools/gf.sh` |

### What the standard still asks for

1. **Citations that give no label.** 542 citations are in module notes or inside function bodies,
   so `cite_labels.py` cannot attach them to an item. Move each one onto the item that ports the
   function. Gate: the quick tier, and more labels in `names-from-port.tsv`.
2. **Fields of the main classes.** `labels/fields.tsv` is almost empty. Recover the fields of
   Citizen, Building and Storage with GameFrame's `decomp_query.py layout`, with evidence for each
   field. Gate: each row cites the functions that read or write the field.
3. **Library code.** Some of the 9,846 functions in the Application are the C runtime and the
   C++ standard library. GameFrame's guide on compiled code shows how to name them by signature
   (Ghidra's Function ID). Naming them separates the game's own code from library code in every
   coverage figure. Gate: a count of the functions named this way, in CHECKLIST.md.
4. **One coverage figure.** CHECKLIST.md and the case study say 68 % of the game's code is traced.
   Labels cover 28.2 %. The two figures measure different things (`coverage.py` counts citations,
   the labels need an item). Define "traced" once, and report one figure from one tool.
5. **GameFrame's case study** still says `--pure`. That is a fix in GameFrame, not here.

## Part 2: the code

### Measurements

| Measure | Value |
|---|---|
| Rust code | about 141,000 lines: `apps/game` 44,300, `crates/game` 32,100, `crates/sim` 14,700, `crates/formats` 12,800, `crates/wma` 10,600 |
| Files over 800 lines | 31. The largest: `crates/wma/src/tables.rs` 9,318 (data), `apps/game/src/tools/placement_tests.rs` 4,546, `apps/game/src/gameplay/tests.rs` 1,899, `crates/formats/src/savegame_schema.rs` 1,888, `crates/game/src/nature/wildlife.rs` 1,593, `crates/game/src/people/life.rs` 1,441 |
| Clippy | It stops at `unbanished-core`: six `approx_constant` errors on the C runtime's constants, which are exact on purpose. With that rule allowed, it finds 89 warnings and one more error in a test. Clippy is in no test tier |
| Formatting | `cargo fmt --check` is clean |
| `unsafe` | none |
| `TODO`, `FIXME` | none |
| `#[allow(dead_code)]` | 50, most of them for code that only some feature sets use |
| `unwrap()` and `expect()` | about 500 outside test files. The count includes test modules inside source files, so the real number is lower |
| Duplicate dependencies | 18 crates in two versions: `objc2` 0.5 and 0.6 with its family, `bitflags` 1 and 2, `getrandom` 0.2 and 0.4, `syn` 2 and 3, `miniz_oxide`, `rustc-hash`, `block2` |
| Renderer | 11 render pipelines and the model shader are in `apps/game/src/gfx` (8,300 lines). `crates/gfx` has 821 lines. The viewer draws models with its own shader (`apps/viewer/src/mesh.wgsl`) |
| CI | Tests run only when started by hand. Nothing runs on a push or a pull request, except the website |

### Steps

Each step is one session, or a few. The order puts the cheap checks first, because they protect
every later step.

**1. Clippy as a gate.** Allow the lints that conflict with exactness where they apply, with a
comment that gives the reason: `approx_constant` and `excessive_precision` on constants copied
from the original, `neg_multiply` and `nonminimal_bool` where the original's order of operations
is kept. Fix the other warnings. Add `cargo clippy --workspace --all-targets -- -D warnings` to the
quick tier. Gate: compare.sh identical, and Clippy clean.

**2. CI on every push and pull request.** Run formatting, Clippy, `cargo test` and the browser
build on every push to `main` and on every pull request. Tests that need game data already skip
without it. Several sessions push to `main` at once, so this catches a broken build within
minutes. Gate: the workflow passes on `main`.

**3. Dead-code allowances.** Replace each `#[allow(dead_code)]` with the `cfg` of the features
that use the item, or remove the item. Gate: the features suite builds every feature set with
no warnings.

**4. Large files.** Split the files over 800 lines by topic with `tools/refactor`
(`split_tests.py`, `split_impl.py`). The code moves and is not rewritten. Start with the test
files (`placement_tests.rs`, `gameplay/tests.rs`), then `wildlife.rs`, `life.rs`, `town/load.rs`
and `ui/input.rs`. Data tables such as `wma/src/tables.rs` stay as they are. Gate: compare.sh
identical, and the same number of tests.

**5. A renderer crate.** Move the renderer (`apps/game/src/gfx`: pipelines, models, terrain,
decals, shadows, particles) into `crates/gfx`, or into a new `unbanished-render`. The game keeps
the scene and the interface. The viewer then draws models with the game's own shader and
materials, so a model looks the same in both. Gate: compare.sh identical (frames included), and
viewer captures before and after, compared with `imgdiff`.

**6. Errors for the player's files.** Find the `unwrap()` and `expect()` calls on paths that read
the player's files (packages, saves, mods, settings, rule hooks). Return an error there, and show
it to the player or write it to the log. Keep panics for broken invariants inside the simulation.
Gate: a test for each path with a damaged file, and compare.sh identical.

**7. Duplicate dependencies.** Find which crates pull the older versions (`cargo tree -i`).
Update them where a newer release exists. Gate: compare.sh identical, and the size of the
browser build before and after.

**8. GameFrame's open items.** Items 1 to 4 of Part 1, in that order. Gate: as given there.

## Decisions

- **Shared crates.** GameFrame publishes crates that came from here (`gameframe-wma`,
  `gameframe-msvc-crt`). Un-Banished keeps its own `crates/wma`, `crt` and `crt_qsort_by`. The two
  develop on their own, with no check between them.
- **Clippy.** Warnings fail the quick tier (`-D warnings`).
- **CI.** It runs on every push to `main` and on every pull request.

## Open questions

- How many of the 9,846 functions are library code? The answer changes every coverage figure.
- Does the viewer need the game's materials (snow, seasons, damage), or only its models?
