# The emulator harness

To be sure that a port gives exactly the original's results, Un-Banished runs the original
functions themselves, in an emulator, and compares. This page explains how the harness works.
The harness is `tools/emu/pe_emu.py`; [Emulate for exact numbers](../develop/guides/EMULATE.md)
explains how to use it.

## What it does

An emulator is a program that behaves like a processor. The harness uses
[Unicorn](https://www.unicorn-engine.org), a CPU emulator library, to run x86-64 code from the
original Windows programs on any computer, including a Mac. The game itself never starts. Only
one function runs, many thousands of times, with inputs that the script chooses.

To make that possible, the harness:

1. **loads** `Application-steam-x64.exe` and `Runtime-steam-x64.dll` into emulated memory, at the
   addresses they were built for (`0x140000000` and `0x180000000`), so no addresses need fixing;
2. **links** the game's imports to the engine's exports, as Windows would do when it starts the
   game;
3. **replaces** the operating system's functions (memory, threads, files) with small stand-ins,
   and replaces the engine's memory allocator with a simple heap;
4. sets up a stack and thread-local storage;
5. **calls** a function with the Windows x64 calling convention: the first four arguments in
   `rcx`, `rdx`, `r8` and `r9` (or `xmm0` to `xmm3` for floats), and the result in `rax` or
   `xmm0`.

A script then builds the smallest set of objects the function needs, at the field offsets found
by tracing, calls the function for many inputs, and writes the results to
`game-data/derived/emu/`. That folder is on your computer only; it is never committed, because
it is output of the original program.

## How the results are used

A Rust test in the repository reads those results and compares the port with them, bit for bit.
If the folder does not exist (for example on a computer without the game), the test prints a
message and skips. For example:

| Script | What it records | Test |
|---|---|---|
| `terrain_stages.py` | The height grid after each terrain stage | `terrain_emu` |
| `terrain_query.py` | Ground heights, normals and cell types | `terrain_emu` |
| `start_point.py` | The start point of new towns | `terrain_emu` |
| `names.py` | Citizen and town names | `names_emu` |
| `astar.py`, `regions.py`, `sectors.py` | Paths, regions and path-rank distances | `astar_emu`, `regions_emu`, `sectors_emu` |
| `candidate_sort.py` | The order of the citizens' choices | `candidate_sort` |
| `growth.py`, `time.py`, `game_loop.py` | Tree growth, time, the order of one iteration | `growth_emu`, `time_emu`, `game_loop_emu` |
| `crt_math.py`, `transform.py`, `picking.py` | The C runtime's float functions, transforms, picking | `crt_math`, `transform`, `picking_emu` |
| `rng_trace.py` | Every random draw of a function, with its call site | compared with `tools/triage/rngdiff.py` |

There are about 30 such scripts.

## When a function is hard to run

- **A missing pointer.** If the function reads a field the script did not fill, the emulator
  stops with "unmapped memory" and the address. The disassembly shows which object and offset it
  was.
- **Threads, Direct3D, files.** Functions that call them are *hooked*: the script replaces the
  call with Python code that returns a suitable value.
- **Very long functions,** such as the river builder: the script runs the whole function with a
  limit on the number of instructions, and records stage outputs to compare piece by piece.

## Why it matters

Reading decompiled code is not enough for numeric work. Ghidra's pseudocode can hide the float
precision, the order of operations, truncation and loop bounds, and any of these can change the
last bit of a result. Running the original settles these questions. The port is right when its
results match. The first mismatch shows where it is wrong.

## Read more

- [How it was made](../PROCESS.md), step 4: the method from the beginning.
- [Exact maths and random numbers](EXACT-MATHS.md): what needs exact results, and why.
- `tools/triage/README.md`: the random-draw tracing.
