# Roads

Port: `crates/game/src/road.rs` (planning, BuildRoad, CompleteRoad, taking a plan back), the
road mode of `apps/game/src/removal_tool.rs` (RoadTool and Remove Roads) and ClearArea's road
cells (`Sim::clear_try`). Test: `roads_are_built_upgraded_and_removed`
(`apps/game/src/placement_tool.rs`).

## Cells

| Byte | Meaning |
|---|---|
| `+0x29` | the cell type: stone road 0, dirt road 1 (the road templates' MapDescription `+0x20`); a cleared road goes back to 2 |
| `+0x2a` | the road kind the cell has or is planned to have: 0 stone, 1 dirt (`CreateRoadDescription +0x20`), `0xff` none |
| `+0x2b` | `0x80` while a road is being built there, `0xff` once built, 0 cleared |
| `+0x28` | one more for a planned road on a cell that was not a road (placements need it 0) |

Map `+0xe8` keeps the planned cells and their BuildRoad works. Road cells cost 3 in the citizens'
path grid (types 0, 1) and are walked at the CharacterDescription's road speeds (types 1 and 9
at `+0x7c`, type 0 at its own).

## RoadTool

`RoadTool : DragTool` (vtable `0x140168290`, constructor `FUN_14010aca0`) for DirtRoad and
StoneRoad (`CreateRoadDescription`, serializer `FUN_14011f150`: `+0x20` kind, `+0x28` the raw
material each cell takes: none for dirt, stone for stone).

- Start (`FUN_14010abb0`): the template's CreateRoadDescription (`+0x38`).
- Input (`FUN_14010ab60`): DragTool's, and `+0x40` set while key `0x16` is not held.
- Check (`FUN_14010a4b0`) and apply (`FUN_14010a200`) between the drag's two cells (sorted):
  neither end of a type in `0x5e0` (water, high and rough ground) nor occupied (`+0x00`); a path
  from the lower to the higher cell (`FUN_1400a22d0`: every type costs 1, no diagonal steps,
  the Manhattan heuristic with `+0x40`, the blocked bit honoured, over `0xa17` plus the ends'
  types for the check, `0xa17` alone to apply). Apply registers the road kind's decal
  (`FUN_1400b2260`), plans every cell of the path (`FUN_1400a1e30`) and rebuilds the road mesh
  (`FUN_1400b4840`).

**Planning a cell** (`FUN_1400a1e30(cell, kind, type, material)`): only a better kind (lower
`+0x2a`). A cell already planned is taken back first (`FUN_1400a1bd0`). A cell that is not a road
(types 0, 1) and has no kind or plan counts one more on `+0x28`. A BuildRoad work (constructor
`FUN_1400ea020`) is registered with mask `0x2000` and the cell keeps kind and `+0x2b = 0x80`.

## BuildRoad

Vtable `0x140167430`; fields `+0x48` the builder, `+0x50` the material, `+0x58` the cell, `+0x5c`
the type it becomes, `+0x60..+0x68` the kind, `+0x2b` byte and type the cell had. Its profession
(slot `+0x50`, `FUN_1400e9e30`) is the builder's (`Game +0xb0 → +0x38`).

Try (`FUN_1400e9660`): one builder; the cell reachable. On the cell a pile has its Pickup work
offered; a natural resource no one removes is removed from the nearest free neighbour (`0xe07`)
(RemoveNaturalResource sequence). Otherwise the cell is built (`FUN_1400c54a0`): a dirt road at
once; a stone road with a stone carried, or one fetched from the nearest storage holding one
(the builder drops off first when it cannot take one). The sequence: (walk to the storage,
clip 6, take what it can carry of the material, clip 7) → ShowTool(1) → walk to the cell →
PerformWork clip 10 for 4 s, not turning → (use one of the material) → ShowTool(−1) →
CompleteRoad(cell, type, `0xff`) → CompleteTask with parameter 1. Complete (`FUN_1400e9620`)
ends the work on parameter 1; fail, cancel and refresh (`FUN_1400e95f0`, `FUN_1400e9de0`,
`FUN_1400e9590`) keep the builder.

**CompleteRoad** (vtable `0x140165f80`, start `FUN_1400cad20` → `FUN_1400a1b10(cell, byte,
type)`): to type 2 the cell loses its road (`+0x28` one less, `+0x2a = 0xff`, `+0x2b = 0`);
otherwise `+0x2b` takes the byte and the terrain is flattened under the cell (`FUN_1400a35b0`).
The cell takes the type, leaves the planned set, and the road mesh is rebuilt.

## Taking a road back

`FUN_1400a1bd0(cell)`: a planned cell's BuildRoad goes and the cell gets back the kind, `+0x2b`
and type it had (and `+0x28` one less when it had no kind): true unless that left no kind. A cell
without a kind becomes ground (type 2) at once: false. A built road stays: true.

**Remove Roads** (RemovalTool `+0x40`, `FUN_140108ab0`): on a road cell (types 0, 1), a cell
with a kind or a planned one, `FUN_1400a1bd0`; when it answers true a removal marker
(`Template\Clear.rsc`) is placed on the cell. The marker's ClearArea clears road cells
(`ClearAreaDescription +0x29`): `FUN_1400a50d0` lists them (no object, a road type, a kind or a
plan), `FUN_14011d910` is not clear while one remains (`FUN_1400a2d30`), and its try (after the
natural resources) sends a worker to the nearest reachable one: `FUN_1400c54a0` to type 2, the
task's parameter the cell.

## Port notes

- The tool's planned cells and Remove Roads reach the simulation as commands (`PlanRoad`,
  `ClearRoad`), applied before the next iteration; markers are placed by the town after the
  commands run (a frame later than the original).
- The zones' blocked bits are taken from the cells' occupants for the road's path.

## Not ported

The tutorial's fixed drag (DragTool `+0x80`) and the CreateBar's road cost. (The decals follow
`road_revision`; a finished road flattens the terrain at its cell's four corners through
`Construction::flatten_requests`, applied after the iteration.)

## The road decals (`FUN_1400b4840`)

The map rebuilds its tiled decals when a road changes. Per 32-cell tile (`+0xc8`) and road kind
(the map's `+0x210` materials: the road templates' DecalDescriptions, `StoneRoad` kind 0 and
`DirtRoad` kind 1) every cell with that kind (`+0x2a`) gets a quad: its corners
(`FUN_1400b1f70`) on the height grid (wider than the cells by `+0xc0 − +0xc4`), each with the
height, the cell's built byte (`+0x2b`, `0x80` while being built) as alpha, the terrain's uv
(`× 16384 / +0xc0`) and normal (`FUN_1400b3740`); its atlas tile (`FUN_1400b3650`,
`DAT_1401ca450` by the mask of road neighbours: left, right, up, down, then each corner with both
its edges; `DAT_1401ca2d0` the tiles' uv) in the second uv; the diagonal alternates with the
cell's parity (`0 3 1 1 3 2`, odd `0 2 1 0 3 2`). The same pass counts each kind's built cells
(`+0x220`, achievement statistic 23).

Port: `apps/game/src/road_decals.rs`, rebuilt when `Cells::road_revision` changes.
