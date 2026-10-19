# Tunnels

Port: the bridge tool (`apps/game/src/start/bridge.rs`, see [BRIDGES.md](BRIDGES.md)) places
them; `crates/game/src/world/terraform.rs` (TerraformComponent) and
`StartingTown::apply_terraforms` reshape the terrain. Test: `a_tunnel_goes_through_a_hill`
(`apps/game/src/tools/placement_tool.rs`).

## The tool

`Template\Tunnel.rsc` has a CreateBridgeDescription, so the BridgeTool places it like a bridge:

| Field | Tunnel | Meaning |
|---|---|---|
| `+0x20` | `0x440` | spans high (6) and rough (10) ground |
| `+0x28` | `0x17` | also drawn valid |
| `+0x2c` | `0x40` | at least one high cell |
| `+0x24` | `0x1f` | banks |
| `+0x30` | `0x457` | the cells beside it (roads, ground, resources, high, rough) |
| `+0x34` | 1 | one cell to each side |
| `+0x38` | | TunnelStart, Tunnel1, 2, 4, 8 |

The tunnel entity (MapDescription type 6, never stamped) covers the span three cells wide; its
pieces (type 11, stamped when the ClearArea is done) are walkable.

Its ClearAreaDescription (`Template\Tunnel_cleararea`) sets the instant flag (`+0x2d`). When
clearing starts (`FUN_14011ce30`), `FUN_14011d230` removes every other entity on the span's
cells at once (`FUN_1400c8370`). So trees and rocks in a tunnel's path vanish without a
clearing work, and its supply starts on the first update. Mods can set the flag on any
building.

## TerraformComponent

TerraformDescription (serializer `FUN_140058000`): `+0x20` the floor picking mesh with the
cell types `+0x30` it may change, `+0x28` the ceiling mesh with `+0x34`. TunnelStart has
TunnelFloor (`0x457`) and TunnelCeiling (`0x440`); the tunnel entity has neither. The
component (vtable `0x1401609a0`) serializes its kept points (`+0x28`, count `+0x30`).

**Entity event 4** (the build is done, BuildExit8; the MultiObject passes it to the pieces,
`FUN_14012a960`) runs slot `+0x60` (`FUN_140057c70`):

1. `FUN_1400574b0(1)`, the ceiling: every vertex of the mesh's first submesh, through the
   model's transform, a quarter cell on (+0.25, +0.25). Where the cells at it, one west, one
   south-west and one west again (the original repeats it) are all of `+0x34`, and the ground
   under it (`FUN_1400a48b0`) is below the vertex, the height sample under it (terrain `+0xd8`,
   `× 256`) takes the vertex's height and the ground point is kept (`+0x28`);
   `FUN_1400b0620` marks the terrain's chunks for rebuilding.
2. `FUN_1400574b0(0)`, the floor: the same with `+0x30`, lowering where the ground is above
   the vertex; nothing is kept.
3. `FUN_140057a80`: every rough cell (10) of the MapComponent becomes high ground (6) (the
   sectors and path grid told) and is kept (`+0x38`).

**Removal** (slot `+0x38`, `FUN_140057cb0`): the kept points' heights are written back and the
kept cells become rough again. The dug floor stays.

## Port notes

- The heights belong to the terrain, outside the simulation: event 4 and removal queue a
  request (`Construction::terraform_requests`) and the game applies them to the terrain's
  heights after the iteration, then rebuilds and re-uploads the terrain mesh
  (`Terrain::reshaped`). The original reshapes during the iteration.
