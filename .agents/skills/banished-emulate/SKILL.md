---
name: banished-emulate
description: Run functions from the original Banished binaries under the Unicorn emulator (tools/emu/pe_emu.py) to capture bit-exact ground truth, then port to Rust and prove parity with a test. Use for numeric or procedural logic (RNG, noise, terrain, meshes, queries, placement, name generation, simulation maths) where the 1:1 port must match the original output exactly.
---

# Emulate for ground truth, port until bit-exact

This is the method that produced the verified Random, Perlin, terrain generator, surface image,
tile mesh, ground ray cast, cell types and start point. Use it whenever "close enough" isn't.

## Setup

```bash
python3 -m venv .venv && .venv/bin/pip install unicorn pefile   # once
```
`pe_emu.Emu(exe, dll)` loads `Application-steam-x64.exe` and `Runtime-steam-x64.dll` at their
preferred bases, links exe imports to DLL exports, stubs OS imports and replaces the engine
allocator. API: `malloc(size)`, `write(addr, bytes)`, `read(addr, n)`, `u32/i32/u64/f32s`,
`call(addr, *args)` (Windows x64 ABI, returns `rax`), `call_f32` (returns `xmm0`),
`hook_function(addr, name, handler)` to stub a callee.

## Steps

1. **Trace first** (`banished-trace`): know the function's inputs — `this` layout, globals it reads
   (`DAT_…` pointers such as the game singleton at `0x1401cbdd8`), and callees that touch the OS,
   threads, files, rendering or audio (those must be hooked/stubbed).
2. **Write a generator** `tools/emu/<topic>.py` modelled on `terrain_query.py` / `start_point.py`:
   - module docstring with `Usage:` and what files it writes;
   - build the minimal object graph in emulator memory (`malloc` + `struct.pack` at traced offsets);
   - call the function across many inputs (all cells, a seeded grid of random positions, every
     seed in a small range) — cover edge cases (map borders, zero, negatives);
   - write raw little-endian outputs to `game-data/derived/emu/<topic>/` (git-ignored). Never write
     ground truth into tracked paths.
   ```bash
   .venv/bin/python tools/emu/<topic>.py game-data/derived/emu/<topic>
   ```
3. **Port** into `crates/game` (pure logic, no wgpu/winit). Mirror the original's arithmetic
   precisely: f32 vs f64, truncation vs rounding (`cvttss2si` → `as i32`), operation order (float
   addition is not associative), integer wrap (`wrapping_*`), signed shifts. Read the disassembly
   for these — Ghidra pseudocode often hides them.
4. **Parity test** in `crates/game/tests/` (see `terrain_emu.rs`): load the ground-truth files,
   **skip with a message if `game-data/` is missing** (CI and other machines have no game files),
   compare exactly (`assert_eq!` on bits, `f32::to_bits`), and on failure report the first
   mismatching index plus a count.
   ```bash
   cargo test --release -p unbanished-game --test <name>
   ```
5. If not exact, bisect: emulate intermediate callees separately (`terrain_stages.py` dumps each
   generator stage) and compare stage by stage.

## Pitfalls

- Unmapped memory access → a pointer field you didn't populate; the harness reports the address.
  Find which offset of which object it came from in the disassembly.
- Functions that allocate via the engine heap work; ones that use TLS need `GS_BASE` (set up by
  the harness), ones that spawn threads or call D3D must be hooked.
- Very long functions (e.g. the river strip `FUN_1400d4fd0`): pass `timeout_insns` and emulate
  the whole thing rather than hand-porting blind, then port piecewise against stage dumps.
- Record in the doc which generator script and which inputs proved parity, and the match rate
  if not 100% against real saves (e.g. "99.95% vs allportez.sav").
