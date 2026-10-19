# 2. Emulate for exact numbers

Some behaviour must match the original to the last bit: the random number generator, the noise,
map generation, the terrain mesh, ground queries, name generation and much of the simulation's
maths. Reading the code is not enough for this. One wrong rounding step changes the map.

So Un-Banished runs the original functions in an emulator, records their output for many inputs,
and tests the Rust port against that output. This method proved the random numbers, the Perlin
noise, the terrain generator, the terrain's surface image, the tile mesh, the ground ray cast,
the cell types and the start point.

## The harness

`tools/emu/pe_emu.py` runs code from the original programs with
[Unicorn](https://www.unicorn-engine.org), a CPU emulator. It loads `Application-steam-x64.exe`
and `Runtime-steam-x64.dll` at their normal addresses, links the program's imports to the DLL's
exports, replaces the operating system's functions with stubs, and replaces the engine's memory
allocator with a simple heap. [The emulator harness](../../topics/EMULATOR.md) explains how it
works.

Set it up once:

```bash
python3 -m venv .venv && .venv/bin/pip install unicorn pefile
```

The main calls:

| Call | What it does |
|---|---|
| `Emu(exe, dll)` | Load both programs |
| `malloc(size)` | Allocate memory in the emulator |
| `write(addr, bytes)`, `read(addr, n)` | Write and read memory |
| `u32`, `i32`, `u64`, `f32s` | Read typed values |
| `call(addr, *args)` | Call a function (Windows x64 convention); returns `rax` |
| `call_f32(addr, *args)` | Call a function that returns a float; returns `xmm0` |
| `hook_function(addr, name, handler)` | Replace a function with your own Python code |

Pass a float argument as `F32(value)`, so that it goes into an `xmm` register.

## Steps

1. **Trace the function first** ([Trace](TRACE.md)). You need to know its inputs: the layout of
   `this`, the global values it reads (for example the game object at `0x1401cbdd8`), and every
   function it calls that uses the operating system, threads, files, graphics or sound. Those
   must be replaced with hooks.
2. **Write a generator**, `tools/emu/<topic>.py`. Use `terrain_query.py` or `start_point.py` as
   a model.
   - Start with a docstring that says how to run it and which files it writes.
   - Build the smallest set of objects the function needs in emulator memory, with `malloc` and
     `struct.pack` at the offsets you traced.
   - Call the function for many inputs: every cell, a grid of random positions, every seed in a
     range. Include the edge cases: the map border, zero and negative values.
   - Write the raw output, little-endian, to `game-data/derived/emu/<topic>/`. Git ignores this
     folder. Never write the output to a tracked folder.
   ```bash
   .venv/bin/python tools/emu/<topic>.py game-data/derived/emu/<topic>
   ```
3. **Port the function** into the right crate ([Port](PORT.md)). Copy the original's arithmetic
   exactly: `f32` or `f64`, truncation or rounding, the order of operations (float addition is
   not associative), wrapping integer arithmetic, signed shifts. The disassembly shows these
   details. The pseudocode often hides them.
4. **Write a parity test** in the crate's `tests/` folder. `crates/game/tests/terrain_emu.rs` is
   a model.
   - Load the generator's output.
   - If the folder does not exist, print a message and return. Other computers do not have it.
   - Compare the bits exactly (`f32::to_bits`), not with a tolerance.
   - On a failure, report the first index that differs and how many differ.
   ```bash
   cargo test --release -p banished-game --test terrain_emu
   ```
5. **If the result is not exact, divide the problem.** Emulate the inner functions on their own,
   and compare each stage. For example, `terrain_stages.py` writes the height grid after each of
   the five generation stages.

## Random-draw parity

Many ported functions draw numbers from the game's random number generator. The draws must come
in the same order and the same number as in the original, or every later number differs.

1. Record the original's draws in the emulator: create `RngTrace(emu)` from
   `tools/emu/rng_trace.py` before `emu.call`, then write the trace.
2. Record the port's draws with `banished_core::random::trace`:
   ```rust
   use banished_core::random::trace;
   trace::start();
   // ... call the port with the same inputs ...
   let draws = trace::take();
   trace::write(std::path::Path::new("port.rng"), &draws)?;
   ```
3. Compare the two:
   ```bash
   python3 tools/triage/rngdiff.py orig.rng port.rng
   ```
   It prints the first draw that differs, with the original's function and offset next to the
   Rust file and line, for example `FUN_1400a9d50+0x216 | crates/game/src/people/names.rs:79`.

`crates/game/examples/rng_trace_names.rs` is a complete example. It traces the names of three
citizens, and all 53 draws match.

## Common problems

- **"Unmapped memory access."** A pointer field that you did not fill. The harness reports the
  address. Find in the disassembly which offset of which object the pointer came from.
- **Thread-local storage.** Functions that use it need `GS_BASE`. The harness sets it up.
- **Threads and Direct3D.** Functions that start threads or call Direct3D must be hooked.
- **Very long functions,** such as the river builder `FUN_1400d4fd0`. Pass `timeout_insns` and
  emulate the whole function. Then port it piece by piece and compare stage outputs.
- **Record the proof.** Write in the topic document which generator and which inputs proved
  parity. If parity against real saves is not 100 %, give the rate, for example
  "99.95 % against allportez.sav".
