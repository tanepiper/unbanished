# Entity selection, picking and highlight

Slice B, Steam x64 1.0.7. Evidence: Application/Runtime/VideoDX11 decompilation and
disassembly, decoded resources, original-function emulation and remake captures. No matching
original-game screenshot was supplied, so visual parity with the original is unverified.
The port uses `apps/game/src/{selection,entity_ui,highlight}.rs` and
`crates/game/src/picking.rs`. Building/citizen controller values and commands belong to the
subsequent slices; this slice opens their packaged hosts and logs the attached controller ids.

## Input and ranking

| Function | Observed role |
|---|---|
| `FUN_140107b70` | ToolManager input/camera/tool dispatch before the simulation update |
| `FUN_140105a70` | Cursor ray: camera origin, frustum screen direction, transformed to world |
| `FUN_140102500` | EditTool left press: terrain lookup, picking scene, selected entity |
| `FUN_1400afa00` / `FUN_1400afef0` | Terrain DDA and cell triangles; output distance clips picking |
| Runtime `0x180126f50` | Picking scene nearest front-facing triangle intersection |
| `FUN_140102880` | Change selection, reuse/open the entity's UIDescription dialog |

The click begins with a 400-unit maximum. Terrain intersection supplies both a map-cell entity
and a distance. A picking mesh hit within that distance replaces the terrain entity. There is
no hard-coded citizen/building/resource class priority in this path. Empty terrain retains the
current selection. Picking uses strict positive determinant (backfaces rejected), inclusive
barycentric edges and inclusive distance limits. A hit at the current best distance replaces
the preceding hit. Terrain and picking have separate triangle routines: terrain permits the
existing `-0.0001` barycentric tolerance; picking uses zero.

`PickingDescription` (`0x541cc42e`, serializer `FUN_14012e1d0`) contains variant/stage mesh
references at `+0x20/+0x28`, a material at `+0x30`, and byte `+0x38` (retained without naming
its meaning). `FUN_14012e420` registers the model's chosen variant/stage and its rigid transform.
Character updates write the root picking transform through `FUN_14010e990`. Runtime AddMesh
`0x180127b70` constructs the picking mesh's own bind hierarchy; this is separate from rendered
character skinning. Picking therefore does not use the animated visual vertices or visual scale.

`PickingMesh` (`0x9c2301f9`, serializer `0x1801251f0`) stores two source/name strings, a byte,
and an array of 0x80-byte submeshes. Submesh serialization (`0x18007b6f0`) is:

| Runtime offset | Content |
|---|---|
| `+0x00`, `+0x08` | name and parent submesh index |
| `+0x0c`, `+0x1c` | quaternion and translation |
| `+0x28`, `+0x34` | local bounds min/max |
| `+0x40/+0x48` | float3 vertices and count |
| `+0x50/+0x58` | float3 normals and count |
| `+0x60/+0x68` | triangles: three u16 vertex indices, then three normal indices |
| `+0x70/+0x78` | u16 endpoint pairs and count |

## Highlight

`FUN_140101eb0` selects a HighlightComponent by setting byte `+0x11c`; `FUN_140102010`
clears it. An entity with a MultiObjectComponent selects each of its parts (`+0x48`) first, so
a pasture's shelter and fence, or a bridge's pieces, light up with it. `FUN_14004ce00` removes the preceding edge instance and registers the chosen
HighlightDescription variant/stage (`+0x20/+0x28`). Model-less entities use the footprint
builder `FUN_14004c290`: ground-cell mask triangles and four perimeter runs. Each perimeter
segment uses the four endpoint/corner vertices and six indices emitted by `FUN_14004bda0`.

HighlightDescription (`0x4a96367e`, serializer `FUN_14004d330`) references `Graphics::EdgeMesh`
assets, followed by mask and edge material references. EdgeMesh (`0x78e68d30`, serializer
`0x1800a3120`) contains five references, two six-i32 range tables, eight vertex/index buffers
and bounds. Mask vertex strides are 8/16 bytes (rigid/skinned); edge strides are 16/32.
The port reads these assets instead of outlining the coarser picking geometry.

Runtime EdgeMeshInstance `0x1800a3f20/0x1800a40a0` draws a stencil mask, then expanded edges.
The shipped SelectionMask/SelectionEdge render states, interpreted through VideoDX11
`0x180003760`, supply:

- Mask: no colour writes, depth always/no writes, clockwise fronts with back-face culling,
  stencil reference 1, replace on pass.
- Edges: alpha blending, depth less/no writes, no culling, stencil reference 0/equal,
  invert on pass. Depth bias is `trunc(-0.0001*16777215) = -1677`; slope bias is
  `-0.0001*4096 = -0.4096`.
- `SelectionEdgeMaterial_Normal_vs` program 0 expands endpoints and sides using a
  camera-right projection differential, inverse viewport width and factor 4.
- Its pixel program writes `(0.99000001, 0.800000012, 0.300000012, 1)`.

The game renderer now uses a depth/stencil attachment and the two selection passes. Selected
citizens use their current four-bone weighted pose for mask/edge geometry; this reads existing
animation state without advancing it. Existing animation blending limitations remain.

## Entity windows

Selection uses `FUN_140102880`, rather than the toolbar-only `FUN_140106f50` entry point.
Both construct windows from UIDescription. Entity resources use `Dialog/StandardDialog.rsc:entity`,
whose controller is EntityUI. Every UIDescription controller element is attached to its named
slot; TabElement slots use the packaged tab label/content pair. `Ui::add_tab` preserves table
order. Completed entities select UIDescription `+0x40` as their active `pages` page
(`FUN_1400676e0`); StorageBarn, StorageYard and Citizen specify page 1.

`FUN_140102270` positions an unpinned window above the entity's projected bounds. Selecting a
different entity closes the prior unpinned host, while pinned hosts remain. Closing and pinning
operate only on presentation state. Titles use the citizen name or the template's localized
toolbar name. Unsupported controller refresh is logged with its numeric id. The shipped storage
tab and capacity/header elements are present, but their live values are pending slice C.

Selection, window creation, layout and highlight generation accept no mutable town or game
Random. Input changes these presentation objects before `FUN_14009b850`'s simulation phase;
there is no simulation command to enqueue for selecting/opening/closing/pinning. No simulation
clock or animation playback clock advances in the view code. Caches are BTreeMap and rows,
controllers, candidates and geometry use explicit Vec/resource order.

## Verification and explicit limits

- Python prototypes consume every byte of 161 PickingMesh, 47 PickingDescription, 150 EdgeMesh
  and 54 HighlightDescription resources. The matching Rust corpus test parses all 412 and checks
  picking index bounds. Tests skip only when local game data is absent.
- `tools/emu/picking.py` / `crates/game/tests/picking_emu.rs`: 500 full original picking-scene
  intersections, stubbing only the spatial candidate enumerator; 200 complete original terrain
  ray casts without terrain/math hooks. Selected ids, hit coordinates and distances match bits.
- The app integration repeatedly selects the barn, builds/draws its packaged host and highlight,
  checks its active page and closes it. Citizen/storage state, Random and calendar are unchanged.
- Acceleration differs: picking candidates use creation order instead of spatial-grid linked
  order, so exactly coincident equal-distance entities can choose differently. Terrain clipping
  scans the ray's cell rectangle instead of DDA; the 200 original cases pass, while traversal
  equivalence at all shared-edge/tolerance cases is not established.
- Start structures still do not stamp the simulation map. Selection derives a read-only fallback
  lookup from their storage footprints until normal registration supplies the cell pointer.
- Offscreen host retirement, dragging and persisted pin settings remain pending. Storage/work/citizen data is intentionally left for C/D.

Capture sequence after the standard seed-1234 New Game/zoom inputs:
`--wheel -12@10 --click-at 450,230@10.5`, capture at 22 seconds. This selects the barn with its
Storage tab and gold outline. Local output: `game-data/derived/captures/ui-selection-barn-final.png`.
