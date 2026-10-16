# Bridges

Port: `apps/game/src/start/bridge.rs` (BridgeTool's check and apply, `StartingTown::place_bridge`),
the bridge mode of `apps/game/src/removal_tool.rs` (the drag and its overlay) and
`crates/game/src/floor.rs` (FloorComponent and the ground on a deck). Test:
`a_bridge_spans_the_river` (`apps/game/src/placement_tool.rs`).

## CreateBridgeDescription

`Template\WoodBridge.rsc:createbridge` (serializer `FUN_14011eaa0`, in this order):

| Field | WoodBridge | Meaning |
|---|---|---|
| `+0x20` | `0x180` | the types spanned (shallow and deep water) |
| `+0x28` | `0x17` | further types the overlay draws as valid |
| `+0x2c` | `0x180` | at least one spanned cell must be one of these |
| `+0x24` | `0x1f` | the banks (types 0–4: roads, ground and the like) |
| `+0x30` | 0 | the types beside the bridge within `+0x34` |
| `+0x34` | 0 | the half width |
| `+0x38` | | five part templates: WoodBridgeStart, WoodBridge1, 2, 4 and 8 |

## BridgeTool

`BridgeTool : DragTool` (vtable `0x140167f20`). Start (`FUN_140100c70`) keeps the description.

- **The end** (`FUN_140100be0`): the cursor's cell moved onto the start's row when it is at
  least as far along x as along y, else onto its column.
- **The check** (`FUN_1401007a0`, from slot `+0x70` `FUN_1401001c0`, which reports the span as
  the CreateBar's size): over the rectangle, row by row, nothing may stand (`+0x00`); the cells
  run banks, span, banks (each at least one cell; any other type fails), and some spanned cell
  is of `+0x2c`. Then from the first spanned cell to one past the span (and `+0x34` to each
  side, of `+0x30` types) no occupant, no road type (0, 1), no road kind and no planned road.
- **The overlay** (slot `+0x40`, `FUN_140100290`): each cell green when the check holds and it
  is of `+0x20 | +0x28` with nothing on it, or, before the drag has any length, of `+0x24`.
- **Apply** (slot `+0x68`, `FUN_1400ff1a0`): a list of pieces: a start piece (one cell) at each
  end; between them the longest piece (8, 4, 2, 1) that fits the inside `L − 2`; then from the
  middle outwards ever shorter pieces filling half the inside on each side. (A span of three
  gets four cells of pieces; the port keeps it.) The bridge entity (the tool's template) takes
  the span's rectangle (`+0x34` wide across), turned 90° along y, at water level (z 0) between
  the first and last spanned cells' ground. Each piece in turn, from the first spanned cell: its
  part template, MapComponent over its cells, turned 0° (90° along y), the first start piece
  180° (−90°), standing at z 0 between its end cells; its PickingComponent's user data the
  bridge (`SetMeshUserData`, so a click selects the bridge); it joins the bridge's
  MultiObjectComponent (whose `+0x58` counts the cells). The bridge is registered last.

The parts' MapDescriptions (type 9, no footprint, not stamped on activation) stamp the bridge
cells when the bridge's ClearArea is done (entity event 1), so builders walk on it while it
goes up; demolished, the parts lift their stamps and the river comes back.

## MultiObjectComponent

Its state table (`FUN_14012b000`, built through `FUN_140036a50` and `FUN_140036b70`) has state 2
only, entered by `FUN_14012aa30`, with a transition to 3. Entering state 2, each part's
MapComponent activates (`FUN_140129680`) and, for a part with a FloorComponent, its floor is added
again (`FUN_140124860` takes a floor already added out first). It returns false, so the earlier
controllers' state-2 entries still run. A bridge's controllers are ClearArea, Build, WorkPlace,
MultiObject and Destroy, in that order. MultiObject is the last that holds state 2, so a built
bridge's machine sits in MultiObject's table at index 0, as saves show.

## FloorComponent

FloorDescription (serializer `FUN_140124b50`): `+0x20` a picking mesh, `+0x28` added on
activation (true for every piece). Activation (`FUN_140124810` → `FUN_140124860`): cell `+0x2d`
one more over the MapComponent's rectangle and the mesh joins the map's picking scene with the
model's transform; removal (`FUN_140124770` → `FUN_1400a2070`) takes both back.

`FUN_1400a48b0` (the ground under a point, unless asked to skip floors): on a cell with
`+0x2d > 0`, a ray from 50 above straight down (at most 100) through the floor scene; the
nearest hit wins when it is above the terrain.

## Port notes

- The tool lives in the drag-tool host (`RemovalTool`): a template with no CreateRemoval or
  CreateRoad description but a CreateBridge one starts it.
- Only the citizens' walking height uses the floors; other ground queries stay on the terrain.

- The CreateBar while dragging shows the span's length and the BuildDescription's
  requirements times it (`FUN_1401001c0`; see [JOBS.md](JOBS.md)). The build stages are the
  pieces' mesh counts (`Site.multi_stages`).
