# World-generation entry path

This note follows the three new-game selection resources into the initial game-loop setup and the
first map-generation helpers. It records static Ghidra pseudocode and decoded shipped data, not
runtime behavior. Binary hashes and analysis setup are in [GAMEPLAY.md](GAMEPLAY.md); the menu
handoff is in [NEW-GAME-FLOW.md](NEW-GAME-FLOW.md).

The first interactive in-game HUD and calendar slice, its static startup trace, and the remaining
game-loop boundary are recorded in [GAMEPLAY-LOOP-FOUNDATIONS.md](GAMEPLAY-LOOP-FOUNDATIONS.md).

## Selector data entering the game loop

The UI list resources contain external references that can be decoded directly from their
`ExternalList` serialization. `FUN_14009f280` loads those lists into the shared game-data context,
and the new-game dialog uses them to populate its selectors:

| UI setting | External list | Referenced resources | Registered item class |
|---|---|---|---|
| Starting conditions | `Game/StartCondition/StartConditions.rsc` | `Hard`, `Medium`, `Easy` | `MapGenerator` (`0xca06d4fd`) |
| Climate | `Game/Climate/ClimateList.rsc` | `Mild`, `Fair`, `Harsh` | `ClimateGenerator` (`0x14c14555`) |
| Terrain | `Game/Terrain/TerrainList.rsc` | `Valleys`, `Mountains` | `TerrainGenerator` (`0xefc2f724`) |

The external-list entries preserve this order; the combo stores each item's list index as item data.
The `.crs` type IDs and Ghidra class registrations agree on those item classes. The current size,
climate, condition, terrain and disaster indices are retained in shared configuration fields at
offsets `+0x24`, `+0x28`, `+0x2c`, `+0x20` and `+0x30`, respectively.

On the new-game path, `FUN_14009d530` builds the child game update-loop object. From the call in
`FUN_1400b9a10`, it stores the town name at `+0x28`, seed at `+0x30`, terrain-size index at `+0x34`,
and the three selected record pointers at `+0x40`, `+0x48` and `+0x50` (starting-condition,
terrain, climate). The integer stored at `+0x15c` is passed as `-1` for this path.

## Fresh-game setup calls

`FUN_14009bfd0` is the game-loop initialization path. When its saved-state pointer is null, it takes
the new-game branch. That branch calls:

- `FUN_1400b5700` with the map-state object, selected `TerrainGenerator`, terrain-size index and
  supplied seed.
- `FUN_1400a87d0` with the selected `MapGenerator` and a newly prepared output structure.
- `FUN_140099300` with `Environment/Environment.rsc` and the selected `ClimateGenerator`.

The `-1` value at `+0x15c` bypasses the branch that constructs an `AchievementInstance` from an
indexed achievement record. This is what the explicit new-game call passes; other entry paths may
use a nonnegative value.

These calls establish the first handoff from UI selections to terrain, starting-condition and
climate setup. They do not yet establish the later per-tick climate rules, the complete order of
world population, or the semantic names of every field in those records.

## Terrain grid setup

`FUN_1400b5700` selects a 0x44-byte record from the `TerrainGenerator` at offset `+0x80`, using the
selected size index. From the first integer in that record it stores two dimension values as `2*n`
and `n`, initializes another grid parameter to `0x20`, and derives `2*n + 1`, `33` and `33²`
working values. It initializes `System::Random` from the supplied map seed and calls
`FUN_1400b1980` with the generated buffer/configuration structure.

`FUN_1400b1980` allocates `(2*n + 1)²` signed 16-bit entries using the derived grid dimension. It
calls `FUN_1400d4860` on its input data, then copies a same-sized float array into the signed 16-bit
grid after multiplying each sample by a constant (`DAT_140178930`) and converting to an integer.
It obtains an additional result from `FUN_1400d4910`, then invokes `FUN_1400b1c50`, `FUN_1400b16b0`
and `FUN_1400b0730` in sequence. It also appends records from counted input arrays into a
dynamically grown collection.

`FUN_1400d4910` seeds a `Utility::PerlinNoise` instance with two values from the same random state
(`RandTol(..., 0xffffff)` for each). It traverses the grid, records each cell's integer grid
coordinates, a float sample, and a three-float vector returned by `FUN_1400d4620`. It then builds a
32-bit value per cell by combining two 8-bit channels derived from threshold tests and Perlin noise;
a later pass adds another byte using normalized neighboring vectors. The exact meanings of those
packed channels and the float sample remain unresolved.

The next calls cross into graphics data: `FUN_1400b1c50` creates an image buffer and uploads the
packed cell values as a texture. `FUN_1400b16b0` creates vertex and index buffers for terrain tiles;
each tile covers a 32-by-32 cell block, and its index pattern alternates triangle diagonals by cell
parity. These calls show how generated grid data is prepared for drawing, while `FUN_1400b0730` and
the terrain material and gameplay consumers remain to be traced. The map-render handoff now shows
that the generated path list is submitted with the River material and a separate point/height-grid
list with the Lake material; see [TERRAIN-WATER-GEOMETRY.md](TERRAIN-WATER-GEOMETRY.md).

The internal five-stage float-grid transform is documented separately in
[TERRAIN-GRID-TRANSFORMS.md](TERRAIN-GRID-TRANSFORMS.md). That trace confirms noise initialization,
path/point-driven spatial passes, a downhill neighbor redistribution, and two neighborhood
averaging passes. The separate path and point collections now trace to River and Lake material
draws, respectively; their evidence is in [TERRAIN-WATER-GEOMETRY.md](TERRAIN-WATER-GEOMETRY.md).

The render-buffer handoff is followed in [TERRAIN-DERIVED-GEOMETRY.md](TERRAIN-DERIVED-GEOMETRY.md):
grid samples feed 32-by-32 terrain tiles, while path and lake-point records produce separate draw
ranges on a second mesh.

The shipped `Valleys` and `Mountains` `TerrainGenerator` profiles and their Small/Medium/Large
integer rows are documented in [TERRAIN-PROFILES.md](TERRAIN-PROFILES.md). Each profile points to
terrain, river, and lake resources; the path and separate lake-point collections feed the matching
River and Lake material draw lists.

The code establishes the buffer sizes, conversion and helper-call order. The signed 16-bit samples
are used as the third terrain-vertex coordinate; the other meanings and units of the scalar field
remain unresolved. Other generated records still need to be traced to their consumers.

## Starting-condition setup

`FUN_1400a87d0` consumes the selected `MapGenerator` record after terrain setup. It derives two
integer values from the record's `+0x3c` field, passes them with the output structure to a map-state
helper, and calls several additional routines (`FUN_1400a7490`, `FUN_1400a6780`, `FUN_1400a62a0`,
`FUN_1400a5eb0`, `FUN_1400a57c0`, `FUN_1400a73d0`). It then iterates multiple counted arrays in the
record, selects entries with the game-loop random generator, and dispatches them through
`FUN_140096ef0` or `FUN_1400a5fe0`.

This confirms that `MapGenerator` is the start-condition input despite its class name. The exact
profile groups and their current consumers are documented in
[START-CONDITION-PROFILES.md](START-CONDITION-PROFILES.md). The code chooses a terrain-valid start
point, places the profile's explicit structures relative to it, registers raw-material and
randomly selected crop/tree template entries, and for Easy selects one livestock type and places
eight instances near the start. Its household records create two linked Citizens plus a random
zero-to-three additional members per record; see
[START-CONDITION-PROFILES.md](START-CONDITION-PROFILES.md) for the per-setting ranges. The
corresponding serializer is `0x1400a7e50`; the same profile note follows the Perlin-gated
rock/ore/tree seeding pass, while exact template threshold meanings and inventory quantities remain
open.

## Verified ports

The terrain generator, surface image, tile mesh, ground query and natural cell types are ported
bit-exactly (`crates/game`); see [TERRAIN-DERIVED-GEOMETRY.md](TERRAIN-DERIVED-GEOMETRY.md) and
[MAP-CELLS.md](MAP-CELLS.md). The gameplay map is `n × n` cells over the centre of the `2n` terrain.

## Remaining world-generation trace

- Port the river strip builder `FUN_1400d4fd0` (ground truth: `tools/emu/river_mesh.py`) and the
  lake surface builder in `FUN_1400b0730`.
- The distinct River and
  Lake mesh production/draw path is recorded in [TERRAIN-WATER-GEOMETRY.md](TERRAIN-WATER-GEOMETRY.md),
  and the profile-driven rock/ore/tree seed pass in [START-CONDITION-PROFILES.md](START-CONDITION-PROFILES.md).
- Continue from the profile-specific placements into exact household inventory and stock quantities;
  see [START-CONDITION-PROFILES.md](START-CONDITION-PROFILES.md) for the confirmed entry path and
  resource-seeding pass.
- Trace the `PopulationDeer` registration into wildlife creation and population updates; current
  resource links and unconfirmed numeric fields are in [WILDLIFE-POPULATION.md](WILDLIFE-POPULATION.md).
- Follow the capacity-aware raw-material assignments to their matching resource and destination
  component types; see [STARTING-INVENTORY-DISTRIBUTION.md](STARTING-INVENTORY-DISTRIBUTION.md).
- Follow the start-condition ambience emitters into the audio manager and playback lifecycle; their
  serialized placement is documented in [STARTING-AUDIO-EMITTERS.md](STARTING-AUDIO-EMITTERS.md).
- Trace how `ClimateGenerator` and `Environment.rsc` control climate, weather and seasonal updates.
- Establish generation determinism and seed consumption order against a runtime or reproducible
  save comparison.
