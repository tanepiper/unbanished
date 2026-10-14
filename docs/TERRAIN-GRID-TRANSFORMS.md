# Terrain grid transforms

This note records the numeric passes invoked by the first terrain-grid generator. It is based on
Ghidra 12.1.4 pseudocode for the Steam x64 executable identified in
[GAMEPLAY.md](GAMEPLAY.md), not a live game run. The caller, grid sizing and later graphics buffers
are described in [WORLD-GENERATION.md](WORLD-GENERATION.md). The generator's field and record names
remain unknown unless stated below.

## Dispatcher and progress stages

`FUN_1400d4860` sets the working grid width to twice the selected terrain record's first integer,
then runs the following functions in order. It stores the stage number at
`DAT_1401cbdd8 + 0x24` before each group; these are progress values, not semantic names.

| Progress | Calls, in order | Confirmed operation |
|---|---|---|
| 1 | `FUN_1400d3aa0` | Initialize the float grid from two independently seeded Perlin-noise fields and per-cell random values. |
| 2 | `FUN_1400d2560`, `FUN_1400d2620`, `FUN_1400d2430` | Ensure at least one path record exists, append randomized point records, then attempt a configured number of additional path records (up to 1,000 attempts each). |
| 3 | `FUN_1400d2930`, `FUN_1400d3de0` | Add randomized local grid modifications, then rasterize stored point/path records into auxiliary arrays and combine them with the float grid. |
| 4 | `FUN_1400d1550` | Repeatedly walk from random interior cells toward lower-valued neighbors and redistribute part of the difference. |
| 5 | `FUN_1400d4120`, `FUN_1400d1820`, `FUN_1400d1820` | Apply another point/path-driven grid transform, then run the neighborhood averaging transform twice. |

The stage labels above describe observable operations and call order. The terrain-generator record
does not yet reveal whether the path-like records represent watercourses, ridgelines, or another
feature, so this note does not assign those meanings.

## Noise initialization

`FUN_1400d3aa0` allocates a float grid with `(size + 1)²` elements. It consumes four bounded random
values to seed two `Utility::PerlinNoise` instances. Each grid cell then samples both fields at
different coordinate scales and combines them with a value from `FUN_140136930` and fixed
constants. The first field contributes directly and also to a blend weight; the second contributes
to the second field's term. The helper names and constants do not identify the resulting quantity,
so it remains an unnamed scalar field here.

## Path and point record construction

`FUN_1400d1d20` builds a candidate counted record containing 28-byte entries. Each entry begins
with two floats, followed by five copied 32-bit values. In the negative-index branch it creates
randomized endpoints and invokes `FUN_1400d19e0`, which recursively inserts displaced midpoints
until the segments are subdivided. In the indexed branch it chooses an existing record as an
endpoint source and builds another candidate. Before appending, the function rejects candidates
that cross themselves or any already stored record; it returns success/failure.

`FUN_1400d2560` repeats candidate generation until the record collection is nonempty.
`FUN_1400d2430` reads a count at `param_1 + 0xa28 + 0x20` and makes that many further attempts,
giving each candidate at most 1,000 tries. `FUN_1400d2620` appends additional two-float records to
collections at offsets `+0x38` and `+0x48`; its coordinates and values are derived from the grid
size and random ranges in the configuration structure. These layouts support calling the data
path/point records, but not assigning a game-world feature name.

## Grid transforms driven by records

`FUN_1400d43b0`, called by both `FUN_1400d3de0` and `FUN_1400d4120`, visits a bounded square around
a supplied point. It derives a random falloff value per visited cell and updates an auxiliary grid
using either a minimum or maximum operation according to a flag. This confirms that the stored
point and path records influence a spatial field.

`FUN_1400d3de0` initializes three auxiliary float arrays to `1`, `0`, and `1`. It rasterizes the
stored 28-byte records into the first two arrays, processes the additional records at `+0x38` into
the second array, and processes records at `+0x28` into the third. For each cell it writes the
original float-grid value multiplied by all three auxiliary values, then subtracts the second
array. The separate `FUN_1400d4120` pass builds two auxiliary arrays from similar record lists and
writes `first * grid - second` back to each cell.

`FUN_1400d1550` performs a configured number of downhill walks. Each walk starts at a random
interior grid cell and repeatedly moves to the lowest of its eight neighbors while that neighbor is
lower. At each step it transfers a bounded fraction of the height difference between the two cells.
The source shows the neighbor search and update arithmetic; naming this as water erosion would be
an inference and is not established by the code alone.

`FUN_1400d1820` computes a neighborhood average into a temporary grid and copies it back. The
averaging loops cover nearby cells, use a center weight plus a different weight for surrounding
cells, and clamp edge reads to the grid boundary. The dispatcher calls this transform twice in a
row.

## Open evidence needed

- Decode the selected `TerrainGenerator` records and map every numeric parameter/count to its
  consumer before naming the generated features.
- Inspect `FUN_1400d2930` and its full config inputs to identify the additional local modifications
  and records it constructs.
- Trace the transformed float grid into the later integer grid, texture channels, tile mesh and
  gameplay systems to identify height, water and terrain-class semantics.
- Compare fixed-seed runs or saved maps before asserting determinism or reproducing the constants.
