# Starting raw-material distribution

This trace follows the seven `RawMaterial*.rsc` candidates from each start-condition profile into
the initial object-component assignment path. Evidence is Ghidra 12.1.4 pseudocode for the Steam x64
executable and extracted `.crs` profile data. It establishes assignment counts and capacity checks,
but not the user-facing names of the seven resource-flag groups.

## Candidate list and filters

`FUN_1400a87d0` appends each path from the profile's `+0xa0` array to a unique candidate list by
calling `FUN_140097330` on the shared object at `DAT_1401cbdd8 + 0xf0`. All three profiles list the
same candidates in this order:

1. `RawMaterialWood`
2. `RawMaterialStone`
3. `RawMaterialIron`
4. `RawMaterialTool`
5. `RawMaterialPotato`
6. `RawMaterialFirewood`
7. `RawMaterialLeatherCoat`

`FUN_1400a73d0` then calls `FUN_140096b30` seven times on that object. Each call supplies one mask
and one profile integer:

| Profile field | Resource flag mask | Hard | Medium | Easy |
|---|---:|---:|---:|---:|
| `+0xfc` | `0x400` | 150 | 120 | 150 |
| `+0x100` | `0x800` | 70 | 60 | 70 |
| `+0x104` | `0x1000` | 50 | 40 | 50 |
| `+0x108` | `0x200` | 50 | 40 | 50 |
| `+0x10c` | `0x20` | 2400 | 1800 | 2400 |
| `+0x110` | `0x40` | 400 | 200 | 400 |
| `+0x114` | `0x4000` | 40 | 30 | 40 |

The masks filter candidate resource templates using the bit field at child offset `+0x5c`. The
serialized values above are the requested assignment caps passed to the distribution routine; they
are not yet mapped to labels such as building material, food, or clothing.

## Capacity-aware assignment loop

For a mask, `FUN_140096b30` builds the filtered candidate list, then walks the map object's child
components. For each eligible holding component it selects a candidate by successful-assignment
count modulo the number of candidates. `FUN_140132250(component, candidate, 1)` checks whether one
unit fits within the component's capacity. When it does, `FUN_1401324d0(component, candidate, 1)`
increments that candidate's stored quantity by one and the successful-assignment count advances.

The routine repeats the component walk while assignments are still succeeding and stops when it
reaches the profile cap or a full pass cannot add anything. This shows the seven profile integers
control total one-unit assignment attempts for their respective resource masks. They do not specify
one fixed amount for every building: actual assignments depend on which components are present,
their current contents and their capacity.

## Remaining work

- Identify the runtime class names represented by the candidate flag field and destination holding
  component type.
- Map each of the seven masks to its matching subset of the seven `RawMaterial` templates.
- Trace the destination objects and component capacities back to the placed starting structures.
- Confirm whether later new-game initialization changes these quantities before the player receives
  control.
