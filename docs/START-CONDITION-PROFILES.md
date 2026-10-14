# Starting-condition profiles

This note follows the shipped `Hard`, `Medium`, and `Easy` `MapGenerator` resources into the fresh
game initialization path. The evidence is extracted `.crs` data and Ghidra 12.1.4 pseudocode for the
Steam x64 executable; it is static analysis, not runtime observation. The top-level handoff is in
[WORLD-GENERATION.md](WORLD-GENERATION.md).

## Profile data shared by all three settings

Each resource is an `Object::External` `MapGenerator`, refers back to
`Game/StartCondition/StartConditions.rsc`, and stores the shared `Template/Citizen.rsc` path.
The profile labels and descriptions are string-table keys `HardName`/`HardDesc`,
`MediumName`/`MediumDesc`, and `EasyName`/`EasyDesc`.

The serialized profile also supplies these common template groups:

| Group in serialized order | Templates | Consumer observed in Ghidra |
|---|---|---|
| `+0x78` | `NaturalResourceRock`, `NaturalResourceOre`, `NaturalResourceTree` | `FUN_1400a6780` uses these paths in its procedural resource-seeding pass. |
| `+0x90` | `PopulationDeer` | `FUN_1400a5eb0` loads an internal from each path and passes it to the shared map manager. |
| `+0xa0` | `RawMaterialWood`, `RawMaterialStone`, `RawMaterialIron`, `RawMaterialTool`, `RawMaterialPotato`, `RawMaterialFirewood`, `RawMaterialLeatherCoat` | `FUN_1400a87d0` passes every entry to `FUN_140097330`, which adds it once to a deduplicated pointer list. |
| `+0xb0` | `NaturalResourceWheat`, `Potato`, `Cabbage`, `Corn`, `Squash`, `Bean`, `Pumpkin`, `Pepper` | The profile's `+0xe0` count chooses random entries and adds them to a deduplicated list via `FUN_140096ef0`. |
| `+0xc0` | `NaturalResourceAppleTree`, `CherryTree`, `PlumTree`, `PeachTree`, `PearTree`, `WalnutTree`, `ChestnutTree`, `PecanTree` | The profile's `+0xe4` count chooses random entries and adds them through `FUN_140096ef0`. |
| `+0xd0` | `LivestockChicken`, `LivestockCow`, `LivestockSheep` | The profile's `+0xe8` count chooses random types; `FUN_1400a5fe0` creates and places the configured number of instances. |

`FUN_140097330` and `FUN_140096ef0` only establish unique-list insertion. They do not, by
themselves, prove that every entry is immediately spawned as a world object.

## Procedural rock, ore, and tree seeding

`FUN_1400a6780` consumes the `+0x78` list one template at a time and scans the map grid for
candidate cells. For each template it creates Perlin-noise fields from the game-loop random
generator, then gates candidates with noise thresholds and random rolls. It skips cells already
occupied in the relevant map-state grid and rejects candidates inside the configured exclusion
radius around the chosen starting point. Terrain queries then check resource-specific height and
surface values before the function clones the resource template and registers the new internal with
the map manager.

The accepted chain proves that these three template inputs seed spatially patterned map resources;
it does not yet identify the serialized fields as exact density, elevation, slope, or cluster-size
settings. Those numeric meanings still require tracing the corresponding template/resource
descriptions.

## Starting citizen groups

The array at `+0x60` contains four, five, or six `(boolean, integer)` records. Every shipped record
is `(true, 3)`. `FUN_1400a62a0` processes each record by calling `FUN_1400a6510` twice with
different selector values, then writes mutual references between the two resulting Citizen
instances. It chooses an additional-member count from 0 through the record's integer and creates
that many more Citizens. `FUN_14011bd70` links each additional instance into the group's linked
member structure and records its group association. The exact role names for the two selected
variants are not assigned here.

This yields the following static ranges from the `+0x60` household loop:

| Profile | Groups | Citizens per group | Total from this loop |
|---|---:|---:|---:|
| Hard | 4 | 2–5 | 8–20 |
| Medium | 5 | 2–5 | 10–25 |
| Easy | 6 | 2–5 | 12–30 |

`FUN_1400a6510` constructs each Citizen from the shared `Citizen.rsc` path, chooses a randomized
position around the group's start coordinate, and passes the selector, age-related value, and
creation flags into `FUN_14011c550`. The bool field in each `(boolean, integer)` row is serialized
as true, but its gameplay meaning is not established by this call path.

## Profile-specific placed structures

The `+0x80` counted records contain a template path followed by five signed integers. The
serializer (`FUN_14001c020`) and `FUN_1400a7490` show that each record is instantiated and placed
relative to the selected starting position. The first two integers offset the position; the final
integer is used as its rotation. The middle pair participate in placement geometry and remain
unnamed here.

| Profile | Placed template | Five serialized integers |
|---|---|---|
| Hard | `StorageCart` | `3, 2, -1, -1, 0` |
| Medium | `StorageBarn` | `-3, 5, -1, -1, 90` |
| Medium | `StorageYard` | `3, 6, 4, 6, 0` |
| Easy | `StorageBarn` | `-3, 5, -1, -1, 90` |
| Easy | `StorageYard` | `3, 6, 4, 6, 0` |
| Easy | `WoodHouse` | `-7, 5, -1, -1, 0` |
| Easy | `WoodHouse` | `-12, 1, -1, -1, 90` |
| Easy | `WoodHouse` | `-12, -4, -1, -1, 90` |
| Easy | `WoodHouse` | `-7, -9, -1, -1, 180` |
| Easy | `WoodHouse` | `-2, -9, -1, -1, 180` |
| Easy | `WoodHouse` | `2, -4, -1, -1, 270` |

`FUN_1400a7bf0` searches for a valid starting point around the map center, testing an increasing
number of angles and radial distances with the terrain-aware `FUN_1400a7d50` predicate.
`FUN_1400a7490` then uses that point as the anchor for the explicit structures above.

## Random resource and livestock selection

The profile data and `FUN_1400a87d0` call sites give these counts:

| Profile | Random crop-template picks (`+0xe0`) | Random tree-template picks (`+0xe4`) | Random livestock-type picks (`+0xe8`) | Instances per selected livestock type (`+0xf8`) |
|---|---:|---:|---:|---:|
| Hard | 0 | 0 | 0 | 8 |
| Medium | 2 | 1 | 0 | 8 |
| Easy | 2 | 2 | 1 | 8 |

For a selected livestock type, `FUN_1400a5fe0` constructs eight instances, assigns randomized
positions around the start point using profile floats at `+0xec`, `+0xf0`, and `+0xf4`, then
registers them with the map manager. With the shipped values this produces no livestock group for
Hard or Medium and one randomly selected type group for Easy. The crop and tree selections in the
table are registered as unique template entries; their later placement is handled elsewhere and is
not inferred here.

All profiles also include one `PopulationDeer` manager input and the same raw-material candidate
list. The population input's resource chain and limits of the current wildlife trace are recorded in
[WILDLIFE-POPULATION.md](WILDLIFE-POPULATION.md). This trace does not yet identify how the
natural-resource lists become the starting map's actual resource distribution or how inventory
quantities are assigned.

## Remaining trace

- Follow the deduplicated crop/tree lists into their later placement consumers, and the raw-material
  list into inventory initialization.
- Identify the seven trailing integer settings and the fixed boolean/integer records in the
  `MapGenerator` serializer before naming their gameplay effects.
- Trace the manager update path and wildlife entity counts; see
  [WILDLIFE-POPULATION.md](WILDLIFE-POPULATION.md).
