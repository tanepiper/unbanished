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
   *Done:* most of them were on struct fields, enum variants and constants, which the tool did
   not read. It now reads them, and module notes name their module, ranked below every item.
   531 of the 542 give a label; `names-from-port.tsv` went from 477 to 727 labels, and the traced
   share of the Application's own code from 31.3 % to 37.9 %.
2. **Fields of the main classes.** `labels/fields.tsv` is almost empty. Recover the fields of
   Citizen, Building and Storage with GameFrame's `decomp_query.py layout`, with evidence for each
   field. Gate: each row cites the functions that read or write the field.
3. **Library code.** Some of the 9,846 functions in the Application are the C runtime and the
   C++ standard library. GameFrame's guide on compiled code shows how to name them by signature
   (Ghidra's Function ID). Naming them separates the game's own code from library code in every
   coverage figure. Gate: a count of the functions named this way, in CHECKLIST.md.
   *Done:* `tools/triage/library.py` names 292 functions in the Application and 682 in the
   Runtime in `labels/library.tsv` ([LIBRARY-CODE.md](LIBRARY-CODE.md)).
4. **One coverage figure.** CHECKLIST.md and the case study say 68 % of the game's code is traced.
   Labels cover 28.2 %. The two figures measure different things (`coverage.py` counts citations,
   the labels need an item). Define "traced" once, and report one figure from one tool.
   *Done:* traced means labelled. `fnindex.py coverage` reports 31.3 % of the Application's own
   code, with library code apart ([CHECKLIST.md](CHECKLIST.md), "Coverage").
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

**1. Clippy as a gate.** Done (`b94b206`). Allow the lints that conflict with exactness where they apply, with a
comment that gives the reason: `approx_constant` and `excessive_precision` on constants copied
from the original, `neg_multiply` and `nonminimal_bool` where the original's order of operations
is kept. Fix the other warnings. Add `cargo clippy --workspace --all-targets -- -D warnings` to the
quick tier. Gate: compare.sh identical, and Clippy clean.

**2. CI on every push and pull request.** Tried (`896f4e2`) and taken back: GitHub's minutes are
not spent on every push. The Tests workflow now has formatting and Clippy, and runs by hand. Run formatting, Clippy, `cargo test` and the browser
build on every push to `main` and on every pull request. Tests that need game data already skip
without it. Several sessions push to `main` at once, so this catches a broken build within
minutes. Gate: the workflow passes on `main`.

**3. Dead-code allowances.** Done. Of the 50 allowances, 43 mark code that only the native build or
only the browser build uses (`cfg_attr(target_arch = …)`), or that one feature set leaves unused.
They stay: moving such struct fields behind `#[cfg]` would spread `cfg` to every place that
builds the struct. Six of the seven unconditional ones keep the original's full tables and the
stand-in types of a build without multiplayer. The seventh, `anim_audio_events` in the game's
sound code, was left over from before the simulation took over animation sounds, and is
removed.

**4. Large files.** In progress. Split so far: the placement and game-loop tests, `wildlife.rs`,
`life.rs`, `town/load.rs`'s tests, `ui/input.rs`, `seeding.rs`, `removal_tool.rs`, formats'
`ui.rs`, `tutorial.rs`, `utility.rs` and the WMA tables (9,318 lines into eight files). No Rust
file may now pass 500 lines: the quick tier checks it with `tools/test/file_size.py`. The 86 files
over 500 lines on 2026-10-22 are listed in `tools/test/large-files.txt`, the largest first. Each
may only shrink, and its line goes when it is split. Gate for each split: compare.sh identical,
and the same number of tests. On 2026-10-22 every listed file outside `apps/game` and `crates/game`
was split below the limit.

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
- **CI.** Checks run locally with `tools/test/run.py`. The Tests workflow on GitHub runs only by
  hand, so GitHub's minutes are not spent on every push.

## Open questions

- **One iteration's order** ([issue #22](https://github.com/tanepiper/unbanished/issues/22)). `FUN_1400c9560` runs the every-iteration set (bit `0x04`) first,
  then visits buckets 0 to 4 in turn, and runs each visited entity's scheduled components
  (bit `0x08`, slot `+0x48`) together, in their array's order (*observed*). The port runs the
  natural resources' and animals' visits in a pass of their own, then the characters' actions,
  the sites' systems one after another (bucket 3) and the citizens last (bucket 1). Random draws
  come in another order. One dispatcher over the visits, in order, would fix it. It touches every
  result, and nothing here can check it against the original running. Saves of one town made by
  the original a known number of iterations apart (see SAVE-WRITING.md, `--sim-continue`) can
  measure it once vanilla pairs exist: the Driscola pair is modded.

- Does the viewer need the game's materials (snow, seasons, damage), or only its models?
