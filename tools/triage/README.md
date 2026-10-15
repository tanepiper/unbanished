# Triage tools

Deterministic helpers for deciding what to trace, port or check next. All read local data only
(`analysis/` Ghidra exports and the repository); none distribute game files or decompiled code.

| Tool | Question it answers |
|---|---|
| `fnindex.py info ADDR` | What does this function touch (callers, callees, class types, strings) and is it traced? |
| `fnindex.py frontier ADDR --depth N` | Which untraced functions does this feature still depend on, largest first? |
| `fnindex.py hot` | Which untraced functions are called most by already-traced code (best next targets)? |
| `fnindex.py find TERM` | Which functions use a string or a registered class name (`CitizenAIComponent`)? |
| `fnindex.py coverage` | How much of the binary is traced, and which component families are untouched? |
| `deviations.py [--check]` | Every comment marking a deviation or unported piece, as `docs/DEVIATIONS.md` |
| `determinism.sh [SECS] [SEED]` | Do two runs with the same seed render identical frames? |
| `cargo run -p banished-app --example imgdiff -- A.png B.png` | How and where do two screenshots differ (MAE, PSNR, mismatch %, bounding box, 4×4 grid, heat map; `--resize`, `--ignore x,y,w,h`, `--json`)? |

"Traced" means the address appears in `docs/`, `crates/`, `apps/` or `tools/`, so naming the
original function in a doc or a code comment is what marks it done. Small functions that only
look up one component type are labelled `get<Type>` and left out of `frontier`/`hot`.

`imgdiff` is the basis for comparing against screenshots of the original game: capture the same
view in both, pass `--resize` when resolutions differ and `--ignore` for the UI clock or cursor.
