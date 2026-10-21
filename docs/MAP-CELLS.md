# Gameplay map cells

The gameplay map (the object at game-data context `+0xf8`) is a grid of 48-byte cells that
records what each square of the playable area is. This note covers its size, the natural cell
types derived from the terrain, the ground query they use, and the save-file layout. Ports live in
`crates/terrain/src/query.rs`; save reading is `Save::map_cells` in `crates/formats`.

Evidence: Ghidra 12.1.4 pseudocode and disassembly of the Steam x64 executable, emulation of the
original functions (`tools/emu/terrain_query.py`, bit-exact against the Rust port for every cell
of two 512-cell maps and 4,096 random height samples), and `allportez.sav` (Valleys Large).

## Size: the playable area is the centre of the terrain

`FUN_1400a43f0` initialises the map with `n` cells per side, where `n` is the terrain record's
first integer. The terrain grid is `2n` cells wide (`2n + 1` samples), so the playable map covers
only the central `n × n` square; the outer ring is scenery. A Large save stores a 512-cell map
over 1,024-cell terrain.

Cell `(i, j)` is centred at map position `(i - (n/2 - 0.5), j - (n/2 - 0.5))`. Positions are in
terrain cells relative to the terrain centre.

## Ground query (`FUN_1400b6460`)

The game finds the ground height and normal anywhere by casting a ray straight down from
`z = 100` (length 200) at the cell `(trunc(x + n), trunc(y + n))` (`FUN_1400afef0`). It tests the
cell's two triangles with Möller–Trumbore intersection (`FUN_1400afce0`, epsilon `-1e-4`), using
the saved `i16` heights `/ 256`. With corners A `(x, y)`, B `(x+1, y)`, C `(x+1, y+1)`,
D `(x, y+1)`, the triangles match the render mesh: even `(x + y)` gives (A, B, D) then (B, C, D);
odd gives (A, B, C) then (A, C, D). A hit on the first triangle shortens the ray for the second.
The second test writes `t` as soon as its barycentric checks pass, even if `t` is then rejected.
The final height uses that last `t`. The normal is `normalize(e1 × e2)` of the last accepted
triangle. With no hit the height is 0.

## Natural cell types (`FUN_1400a3790`)

Byte `+0x29` of a cell is its type. At map creation every cell gets the type from the ground
query at its centre (`h` = height, `up` = normal z):

| Condition (in order) | Type |
|---|---|
| `h < -1.4` | 8 (deep water) |
| `h < -0.25` | 7 (shallow water) |
| `-0.25 ≤ h ≤ 0.75` and `up ≥ 0.9` | 2 (buildable ground) |
| `h < 2.0` | 10 (rough ground) |
| otherwise | 6 (high ground) |

The type names are inferred from the thresholds and from the masks used elsewhere: types 5–8
(`0x1e0`) block movement and 0–4 and 9–11 (`0xe1f`) are passable. The pathfinder setup
(`FUN_1400a3d90`) gives per-type costs: 0, 1, 9 → 3; 2, 10 → 4; 3 → 8; 4–8 → 16. Types 0, 1, 3,
4, 5, 9 and 11 come from player or object footprints. `FUN_1400a30f0` stamps building footprint
strings, where `_` sets type 2 and `#` sets the building's own type. `FUN_1400a33b0` restores
natural types when a footprint is cleared.

After classification `FUN_1400a1950` flood-fills connectivity region ids into the cell's `u16`
at `+0x26` over passable cells. `FUN_1400a3d90` builds the pathfinder. Ground cells with water
(types 7–8) in their 3×3 neighbourhood are registered with the game-data manager via
`FUN_14008e050`.

Against `allportez.sav`, natural types computed from its stored heights match 99.95% of the
232,548 cells that are unoccupied and of a natural type. All cells under objects that differ are
bridge and dock footprints. The remaining 123 cells are where buildings reshaped the ground, since
types are only recomputed when a footprint is cleared.

## Save layout (`FUN_1400a45d0`, serializer version 4)

The map is a block `0xaaaaaaaa, size, 4` followed by `i32 n`, `i32 n²`, the cells, nine `i16`
values (map `+0x90`) and `0xbbbbbbbb`. Each cell (`FUN_14001b570`):

| Field | Encoding |
|---|---|
| object pointers `+0x00`, `+0x08` | `u8` set flag; if set, `i32, u32, u32` (`InternalBasePtr::Serialize`) |
| external reference `+0x10` | `u32` reference id (`0xffffffff` when unset) |
| `+0x1c..+0x24` | 9 bytes |
| `+0x26` | `u16` (connectivity region) |
| (version < 3 only) | `u16` |
| `+0x28..+0x2d` | 6 bytes; `+0x29` is the type, `+0x2a` is `0xff` when free |

Loading a map saved with serializer version < 2 recomputes natural types for empty cells.
