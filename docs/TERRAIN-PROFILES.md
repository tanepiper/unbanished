# Terrain generator profiles

This note records the serialized `TerrainGenerator` inputs shipped for the New Game terrain
selector. The files are under `game-data/extracted/Game/Terrain/`; the parser and Ghidra evidence
are described in [GAMEPLAY.md](GAMEPLAY.md). It documents stored values and observed consumers,
without assigning meanings to the remaining numeric fields.

## Shared fields and referenced resources

Ghidra identifies `TerrainGenerator` as an `Object::External` subtype. Its serializer is
`FUN_1400b6b60` at `0x1400b6b60`. In addition to the inherited string-table reference, it writes a
display-name string, a boolean, three filename strings, and a counted array of 0x44-byte integer
records. Both shipped profiles set the boolean to true and reference:

| Field order | Resource path |
|---|---|
| First filename | `Terrain\\Terrain.rsc` |
| Second filename | `Terrain\\River.rsc` |
| Third filename | `Terrain\\Lake.rsc` |

This establishes that both terrain profiles name terrain, river, and lake resources. The map-render
call graph binds the generated path collection to River material draws and a separate generated
point/height-grid collection to Lake material draws; see
[TERRAIN-WATER-GEOMETRY.md](TERRAIN-WATER-GEOMETRY.md).

The matching extracted files are `MaterialInstance` resources with named material and texture
links; see [TERRAIN-MATERIAL-RESOURCES.md](TERRAIN-MATERIAL-RESOURCES.md) for those serialized
bindings.

## Size records

`TerrainSize0`, `TerrainSize1`, and `TerrainSize2` resolve through
`Dialog/StringTable_terrainSize.crs` to Small, Medium, and Large. The first integer in each size
record is used by `FUN_1400b5700` as `n`, producing grid dimensions `2*n` and `n`.

The following values are in the serialized order emitted by `FUN_1400211d0` (17 signed 32-bit
integers per record). The serializer writes the integer at struct offset `+0x24` last; therefore
`s16` below is that field, while each other `sN` is at struct offset `4*N`. These are wire values,
not inferred semantic field names.

| Profile | `s0` | `s1` | `s2` | `s3` | `s4` | `s5` | `s6` | `s7` | `s8` | `s9` | `s10` | `s11` | `s12` | `s13` | `s14` | `s15` | `s16` |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Valleys Small | 256 | 50 | 20 | 30 | 4 | 20 | 30 | 3 | 3 | 0 | 2 | 20 | 30 | 40 | 6 | 12 | 4,000,000 |
| Valleys Medium | 384 | 125 | 20 | 30 | 8 | 20 | 30 | 4 | 3 | 1 | 3 | 30 | 50 | 50 | 6 | 12 | 8,000,000 |
| Valleys Large | 512 | 200 | 20 | 30 | 16 | 20 | 30 | 4 | 4 | 2 | 5 | 30 | 60 | 70 | 6 | 12 | 16,000,000 |
| Mountains Small | 256 | 30 | 10 | 30 | 4 | 20 | 30 | 3 | 4 | 0 | 2 | 20 | 30 | 40 | 6 | 12 | 4,000,000 |
| Mountains Medium | 384 | 75 | 10 | 30 | 8 | 20 | 30 | 4 | 4 | 1 | 3 | 30 | 50 | 50 | 6 | 12 | 8,000,000 |
| Mountains Large | 512 | 120 | 15 | 30 | 16 | 20 | 30 | 4 | 4 | 2 | 5 | 30 | 60 | 70 | 6 | 12 | 16,000,000 |

## Fields with directly observed consumers

The generator uses these profile fields at the following code sites:

| Struct offset | Serialized slot | Observed consumer |
|---|---|---|
| `+0x00` | `s0` | `FUN_1400b5700` uses it as `n`; the main terrain grid uses dimensions based on `2*n + 1`. |
| `+0x20` | `s8` | `FUN_1400d2430` uses it as the number of additional path-candidate attempts to accept. Each candidate has at most 1,000 tries. |
| `+0x24` | `s16` | `FUN_1400d1550` uses it as the number of downhill neighbor walks. |
| `+0x28` through `+0x40` | `s9` through `s15` | `FUN_1400d2620` uses these as bounds/counts for randomized point records and as bounds in its random float calculations. Exact setting names remain unknown. |

`FUN_1400b5700` also copies the three profile filename strings to the terrain setup state before
building the grid. The profile differences and the direct integer consumers are now concrete; the
meaning of `s1` through `s7`, and the semantic names for most later fields, still need a consumer
trace.

## Next trace

- Follow the copied `Terrain.rsc`, `River.rsc`, and `Lake.rsc` resources from the terrain setup state
  through material-instance loading and shader binding.
- Identify the remaining profile fields from their call sites, not from the values or their apparent
  correlation with size.
