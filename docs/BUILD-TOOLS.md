# Build tools: placing buildings

This note covers how the original places a building from the toolbar. The evidence is Ghidra
pseudocode of the Steam x64 build (1.0.7), constants from the executable, and decoded template
resources. The port is:

- `crates/game/src/placement.rs` for the rules;
- `apps/game/src/placement_tool.rs` for the tool;
- `apps/game/src/gameplay.rs` for the input wiring.

## Tool classes

Toolbar kind 1 calls `FUN_140108030`. It finds which tool class handles one of the template's
description types, creates it through `Object::Factory`, and attaches the shared settings
(`ToolSetting`). These classes derive from `Tool` (`Object::Internal`):

| Class | Base | Used for |
|---|---|---|
| `PlacementTool` | `Tool` | buildings with a `CreatePlacedDescription` |
| `DragPlacementTool` | `DragTool` | areas such as yards and fields (ported: [below](#dragplacementtool-vtable-0x140167fb0)) |
| `FencedTool` | `DragTool` | fenced areas (ported: [FARMING.md](FARMING.md), `place_fenced`) |
| `RoadTool`, `PathTool` | `DragTool`, `Tool` | roads |
| `BridgeTool` | `DragTool` | bridges and tunnels (ported: [BRIDGES.md](BRIDGES.md), [TUNNELS.md](TUNNELS.md)) |
| `RemovalTool` | `DragTool` | clearing (ported: [JOBS.md](JOBS.md#removal-tools-removaltool--dragtool-vtable-0x140168200)) |
| `EditTool` | `Tool` | editing |

`RotationSetting` is a `ToolSetting`. Its `+0x10` holds 0..3 and is shared between placements.

## DragPlacementTool (vtable `0x140167fb0`)

Ported as the area mode of the drag host (`apps/game/src/removal_tool.rs`), for CropField,
Orchard and StorageYard. Start (`FUN_1401018b0`) keeps the CreateDragDescription (`+0x38`:
`+0x20` cell types, `+0x28 × +0x2c` the largest, `+0x30 × +0x34` the smallest drag, `+0x24`
occupants ignored, `+0x38` the template whose entities may stand in the way) and the
MapDescription (`+0x40`).

- Check (slot `+0x70`, `FUN_140100fb0`): 1 too small, 2 too large, 3 a cell that does not
  admit it (`FUN_1400a4aa0(+0x20, 0, 100, 0, +0x24, …, +0x38)`), else 0; the CreateBar gets
  the width and height.
- Apply (slot `+0x68`, `FUN_140100dd0`): with the check at 0, the clear template's entities in
  the rectangle go (`FUN_140106da0`), the template's entity takes the rectangle, turned 90°
  unless it is wider than long, and is registered (`FUN_1400c9d70`).
- Draw (slot `+0x40`, `FUN_140101310`): every cell green, red when the drag is too large or
  the cell does not admit it.

FencedTool's input (slot `+0x30`, `FUN_1400e20b0`) adds the rotation keys (`0x13`, `0x14`) on
the shared RotationSetting to DragTool's.

## PlacementTool (vtable `0x140168130`)

### Start (`+0x20`, `FUN_140103a50`)

The tool keeps the template's:

- `CreatePlacedDescription` (`+0x28`);
- `MapDescription` (`+0x30`);
- `ModelDescription` (`+0x38`);
- decal description (`+0x40`);
- rotation setting (`+0x48`).

The ghost (`+0x68`) is a `MeshInstance` showing the **last** mesh of variant
`Rand(variant count)`, drawn from the game stream.

### Input (`+0x30`, `FUN_140103770`)

The tool reads these key-map actions:

| Action | Default key | Effect |
|---|---|---|
| StructureLeft (`0x13`) | R | rotation − 1, wrapping |
| StructureRight (`0x14`) | T | rotation + 1, wrapping |
| StructureType (`0x15`) | F | next variant (`FUN_140102e30`) |
| MouseLeft (`5`) | left button | place (`FUN_140102b20`) |

The keys follow the engine's letter codes: A = `0x17` up to Z = `0x30`, so `0x28`, `0x2a` and
`0x1c` are R, T and F. The same slot opens or closes the building's info dialog
(`FUN_140104eb0`).

### Footprint (`FUN_140102ed0`)

- **Cursor cell:** the camera ray is cast against the terrain (`FUN_140106740`, `FUN_1400afa00`).
  The cell is `clamp(int(hit + n/2))`.
- **Size:** `CreatePlacedDescription +0x44 × +0x48`. Rotation 1 or 3 swaps the sides.
- **Centring:** an odd side spans `c − (s−1)/2 .. c + (s−1)/2`; an even side spans
  `c − s/2 + 1 .. c + s/2`.
- **Map edges:** a footprint outside the map is rejected, and the left column must be at least 1.
- **Yaw:** rotation × 90 (`DAT_140178904`).
- **Position:** the midpoint of the ground at the two corner cells, using the cell positions
  without the half-cell offset. Its z is forced to 0.

### Hover (`+0x38`, `FUN_1401033b0`)

- **Ghost:** moves the ghost and places the footprint decal (`FUN_140055d90`).
- **Validity:** tests the footprint with `FUN_1400a2a50`.
- **Message:** shows `ToolbarDescription.messages[valid ? 0 : 1]` (`CreateOk` / `CreateBlocked`)
  through `FUN_140105130`.

Slot `+0x40` (`FUN_140103190`) draws the per-cell overlay (`FUN_140106850`).

### Validity (`FUN_1400a2a50` → `FUN_1400a4aa0`)

Each footprint cell `(i, j)` maps to one character of the `MapDescription` footprint string.
Rows are `w + 1` characters apart, last row first:

| Rotation | Index |
|---|---|
| 0 | `(h−1−j)(w+1) + i` |
| 90 | `i(h+1) + j` |
| 180 | `j(w+1) + w−1−i` |
| 270 | `(w−1−i)(h+1) + h−1−j` |

`w` and `h` are the rotated sides.

The allowed cell types are:

- the `CreatePlacedDescription +0x20` mask, normally;
- `values[char − '0']` from its `+0x30` list, when that list and the `+0x28` string exist.

For example, the barn uses the string `10000001…` with values `[20, 23]`: types {2, 4} inside and
{0, 1, 2, 4} on the edge columns. Type 4 (natural resources) is allowed; the clear-area work
removes those resources later.

Each cell must then pass `FUN_1400a4aa0`:

- its type is in the mask;
- there is no occupant at `+0x00`, or the occupant is the `Clear` template (`+0x50`);
- the byte at `+0x28` is zero, unless the footprint character is `.`;
- the ground normal z is at least 0 and the height is within 1000 of 0, so this always passes.

### Placing (`FUN_140102b20`)

1. Re-check validity.
2. Remove `Clear` markers in the rectangle (`FUN_140106da0`).
3. Construct the entity and set its map component: rectangle, then rotation in degrees.
4. Transform the model to the position and yaw. Its variant is the tool's, its mesh is −1
   (none).
5. Register it.
6. Advance the ghost's variant.

When registered without the instant flag, the build component (`FUN_14010cfb0`) shows mesh 0, the
first construction stage. Start structures are built instantly instead (see
[RESOURCE-SEEDING.md](RESOURCE-SEEDING.md)). The footprint is stamped into the map
(`FUN_1400a30f0`) later, from a callback (`FUN_14012aa30`).

## CreateBar (`Dialog/CreateBar.rsc`, controller `BuildRequirementUI`)

`FUN_140104eb0` opens the bar while the tool is active. The controller's OnOpened
(`FUN_14005f120`) fills it from the template:

- `imageIcon`: the `ToolbarDescription` sprite;
- `textTitle`: its name (`+0x38`);
- `labelBuild0..N`: one per `BuildDescription` requirement (serializer `FUN_14010e610`: list of
  `(count, raw material)`, work `+0x30`, multi flag `+0x34`). Each shows the raw material's
  small sprite (`+0x20` sheet, `+0x38` name) and the count as `@0`;
- `labelBuildMulti`: shown when the multi flag is set.

The wood house requires 16 wood and 8 stone.

Each hover, `FUN_140105130` updates the bar:

- `textSize` becomes `"%ix%i"`;
- `textStatus` becomes `UI::String(table, messages[!valid])` with the building's lowercase name
  as `@0`. It is hidden when that string is empty;
- `imageIcon` is tinted white when valid and `0xffff0000` (red) when not.

The tool also projects the ghost's bounding-box corners (`FUN_140104080`) to a screen rectangle
(`FUN_140103e30`). The bar goes centred horizontally on that rectangle, with its middle on the
rectangle's top edge. Its upper-left corner moves `0.25` of the way to the target each frame
(`DAT_140178800`, `DAT_140178824`).

## Port status

- **Done:** the full placement cycle:
  - picking the cursor cell;
  - footprint, rotation (R/T) and variant cycling (F);
  - the ghost (finished mesh) following the cursor;
  - original validity rules;
  - left-click placement: the ZoneComponent reserves the footprint (occupants), the building
    becomes a construction site (see [JOBS.md](JOBS.md)) and shows nothing until building
    starts (mesh 0, then the stages);
  - the CreateBar (icon, title, requirements, size, status, validity tint, smoothed position).
- **Deviations:**
  - Picking marches the camera ray against the exact ground query instead of the original
    triangle intersection.
  - Trees and rocks under the footprint stay until the ClearArea work is ported; such a site
    waits in its clearing state.
- **Not done yet:**
  - the footprint decal and per-cell overlay;
  - message strings beyond the bar itself;
  - the drag, path, road, bridge and removal tools;
  - the decal outline. (The ClearArea work, the footprint decal and the terrain flattening,
    `FUN_1400a35b0`, are ported: [JOBS.md](JOBS.md), [DECALS.md](DECALS.md).)
