# River and lake geometry flow

This note connects the terrain generator's two distinct geometry inputs to their render materials.
Evidence is static Ghidra 12.1.4 pseudocode for the Steam x64 executable. It establishes the
engine's data and draw bindings, not a visual runtime capture.

## River path records

`FUN_1400d1d20` appends 28-byte path entries to the generation context's counted collection at
`+0x18` (count at `+0x20`). It builds candidate paths from endpoints and recursively displaced
midpoints, rejecting candidates that intersect existing segments. `FUN_1400d2560` ensures the list
is nonempty, and `FUN_1400d2430` tries the configured number of additional paths.

During mesh construction, `FUN_1400b0730` iterates that `+0x18` path collection and calls
`FUN_1400d4fd0` for each path. It appends each result to a second vertex/index mesh and stores per
path draw ranges in the list at MapComponent `+0x1c0` (count at `+0x1c8`). The MapComponent render
method `FUN_1400b3960` submits that list with the selected `River.rsc` material at `+0x150`.
`FUN_1400b3ee0` handles the corresponding draw flag by binding the path mesh buffers at `+0x160`
and `+0x180` and selecting the draw range from `+0x1c0`.

This producer-to-material chain establishes that the `+0x18` generated path records drive geometry
drawn with the River material.

## Lake point records

`FUN_1400d2620` also creates separate 12-byte records and appends the lake-surface input records to
the generation context's collection at `+0x48` (count at `+0x50`). `FUN_1400b0730` iterates those
records separately. It reads two coordinates and a third scalar; that scalar expands the bounded
grid region examined around the point. It samples the generated height grid and builds vertices and
indices for cells that satisfy its height tests, then stores the result as per-lake draw ranges in
MapComponent `+0x1d0` (count at `+0x1d8`).

`FUN_1400b3960` submits this second list with the selected `Lake.rsc` material at `+0x158`.
`FUN_1400b3ee0` binds the same path-mesh vertex/index buffers but selects lake draw ranges from
`+0x1d0` under a separate draw flag. The profile's Lake material instance points to
`LakeMaterial.rsc`; its texture-slot links are listed in
[TERRAIN-MATERIAL-RESOURCES.md](TERRAIN-MATERIAL-RESOURCES.md).

This establishes a separate point-and-height-grid path from the `+0x48` records to geometry drawn
with the Lake material. It is distinct from the path-record collection used for River draws.

## Render condition and scope

The render method queues both water lists only when the current renderer shader value is not `1`.
The terrain tile list uses the terrain material at `+0xf8` and its own buffer; the river and lake
lists use the secondary mesh buffers and separate draw-range metadata. Resource names, producer
collections, material pointers, and draw submission now agree on the River/Lake split.

This remains static evidence. It does not establish water-flow simulation, navigation blocking,
shoreline rules, or the in-game visual appearance of generated surfaces.

## Ported: the strip and lake builders

Both builders are now ported, in `crates/game/src/terrain/river.rs`, and the gameplay view draws
their output with the profile's River and Lake materials (`World::water`).

### River strip (`FUN_1400d4fd0`)

The strip is built from one path record in four phases.

1. **Bank points.**
   - Every segment's ends are moved by `(+0.5, −0.5)` and the segment goes into a spatial index with 4-unit cells.
   - Every segment but the last places bank points each quarter unit at half width `w`, which is the first point's `params[3]` + 2.
   - Bank A takes the points on the left and bank B those on the right.
   - A point is kept only when no segment in the cells within 1.1·`w` comes nearer than `w` (`FUN_1400d0f00`). That test's projection adds the scalar to both coordinates of `a + d`; the port keeps this quirk.
   - Each joint adds an arc on its outer side: `acos(d·d′)` in degrees ÷ 90 × 20 steps.
2. **Tangents.** Each path point takes the normalized sum of its backward directions to the two neighbours.
3. **Rows.**
   - For each path point, four directions are blended between its tangent and the next point's.
   - Each bank gives the point nearest the path point within twice `w` (`FUN_1400d12c0`). When none is found, the previous index is kept.
   - A row is added only when:
     - both banks have moved past their last row;
     - the line between the two bank points is square to the direction (|dot| < 0.02);
     - one stretch is at least 2 long;
     - the steps are at least 0.001.
   - Both stretches are resampled to `max(len_A, len_B)` points (`FUN_1400d0c20`).
   - `v` runs at 1/32 of the midline's length and carries on across the gaps between rows (`FUN_1400d0830`). It starts at −16.
4. **Quads.** Each pair of rows becomes four quads across, with `u` going from 0 to 1 from bank A to bank B. A sorted map shares equal vertices. The indices per quad are `a, c, b, a, d, c`.

The port matches the original bit for bit. The test `river_strips_valleys_small_290` compares
every vertex and index of the four paths against `tools/emu/river_mesh.py`.

### Water mesh (`FUN_1400b0730`)

**Vertices.** Vertices are 16 bytes:
- `i16 x, y`: position × 48;
- `i16 z`: height in units of 1/256;
- `0`;
- `i16 u, v`: × 1024 for rivers, the position × 48 for lakes;
- normal bytes `7f 7f`;
- an alpha byte;
- `0`.

**Rivers.** Path `i`'s vertices sit at `z = −192 − i`, with alpha 255.

**Lakes.**
- Lake `i` (centre `c`, radius `r`) covers the cells within `r + 2` of its centre. The covered range runs from `(int)(c − r − 2)` and stops short of `(int)(c + r + 2)`.
- Each cell's corners take the map heights at the grid centre plus the corner.
- A cell gets a quad (`0, 1, 2, 0, 2, 3`) when two conditions hold:
  - some corner lies at or below −0.75;
  - some corner's alpha `1 − (|corner − c| − r) / 2`, clamped, is above 0. The distance includes the corner's height.
- Vertices are shared within the lake. Lake vertices sit at `z = i − 191`.
- The menu scenes' baked lakes match this format.

**Ranges.** Ranges hold the first index, first vertex, vertex count and index count. Indices are
local to their range.

Not ported: the water's role in `FUN_1400b1980`'s ambient emitters beyond the river paths
already used (see [STARTING-AUDIO-EMITTERS.md](STARTING-AUDIO-EMITTERS.md)).

