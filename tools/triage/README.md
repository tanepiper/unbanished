# Triage tools

Deterministic helpers for deciding what to trace, port or check next. All read local data only
(`analysis/` Ghidra exports and the repository); none distribute game files or decompiled code.

| Tool | Question it answers |
|---|---|
| `decomp.py ADDR` | Readable decompilation: calls, globals, float constants, vtables and struct fields named; `CheckPtr` noise and inlined array appends collapsed (`--raw`, `--asm`, `--headless` for functions missing from the export, `--binary Runtime-steam-x64.dll`) |
| `types.py class NAME` | A registered class's parent, type id, factory, size, constructor, subclasses and vtable slots with their lifecycle meaning; `owner ADDR` finds the class a method belongs to; `list --parent Action`. Slot meanings are known for components, actions and `UI::DialogController` (bind, close, refresh, handle event). `controller.py NAME` is a shortcut for `types.py class NAME` |
| `fnindex.py info ADDR` | What does this function touch (callers, callees, class types, strings) and is it traced? |
| `fnindex.py frontier ADDR --depth N` | Which untraced functions does this feature still depend on, largest first? |
| `fnindex.py hot` | Which untraced functions are called most by already-traced code (best next targets)? |
| `fnindex.py find TERM` | Which functions use a string or a registered class name (`CitizenAIComponent`)? |
| `fnindex.py coverage` | How much of the binary is traced, and which component families are untouched? |
| `tools/emu/rng_trace.py` + `rngdiff.py ORIG PORT` | Does the port draw from `System::Random` in the same order as the original? (see below) |
| `skills_check.py` | Do the agent skills still point at real paths, with valid front matter, all listed in AGENTS.md? (the `harness` test suite runs it) |
| `deviations.py [--check]` | Every comment marking a deviation or unported piece, as `docs/DEVIATIONS.md` |
| `determinism.sh [SECS] [SEED]` | Do two runs with the same seed render identical frames? |
| `tools/reference/` (`save.sh`, `windows.swift`, README) | Reference frames of the original game streamed via Steam Remote Play, filed locally for comparison |
| `cargo run -p unbanished-app --example imgdiff -- A.png B.png` | How and where do two screenshots differ (MAE, PSNR, mismatch %, bounding box, 4×4 grid, heat map; `--resize`, `--ignore x,y,w,h`, `--crop-a/--crop-b x,y,w,h`, `--blur R` for streamed references, `--json`)? |

"Traced" means the address appears in `docs/`, `crates/`, `apps/` or `tools/`, so naming the
original function in a doc or a code comment is what marks it done. Small functions that only
look up one component type are labelled `get<Type>` and left out of `frontier`/`hot`.

`imgdiff` is the basis for comparing against screenshots of the original game: capture the same
view in both, pass `--resize` when resolutions differ and `--ignore` for the UI clock or cursor.

## Shared knowledge files

- `names.tsv`: analyst labels for original functions (`address label evidence`). Every tool
  shows them instead of `FUN_` names. Vtable slots of registered classes are named
  automatically (`CitizenAIComponent::slot+0x48`, base defaults as `Component::slot+0x28
  (default, 42 classes)`), and one-type component lookups as `get<Type>`.
- `fields.tsv`: struct fields (`class offset name [pointee class]`). `decomp.py` names
  `param_1 + 0xNN` in that class's methods and follows typed pointers
  (`citizen.description.hungry_urgent`).

Add a row whenever you identify a function or field; it is the cheapest way to make the next
read easier for every agent.

## Random-draw parity

Most 1:1 bugs are draws in the wrong order or count. Record both sides and compare:

```python
# original, under the emulator (see tools/emu/*.py for setting up arguments)
from rng_trace import RngTrace
trace = RngTrace(emu); emu.call(0x1400aa000, ...); trace.write('orig.rng')
```

```rust
// port
unbanished_game::random::trace::start();
/* ... same inputs ... */
let draws = unbanished_game::random::trace::take();
unbanished_game::random::trace::write(Path::new("port.rng"), &draws)?;
```

`python3 tools/triage/rngdiff.py orig.rng port.rng` prints the first divergence with the call
sites on both sides (`FUN_1400a9d50+0x216 | crates/game/src/people/names.rs:79`).
`crates/game/examples/rng_trace_names.rs` is a worked example (identical 53 draws).
