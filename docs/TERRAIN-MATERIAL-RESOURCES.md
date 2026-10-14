# Terrain material resource links

This note follows the three filenames stored in each `TerrainGenerator` through the extracted
resource data. It establishes the referenced asset types and their material inputs; it does not
establish shader behavior. The generated path-to-River and point/height-grid-to-Lake bindings are
now traced separately in [TERRAIN-WATER-GEOMETRY.md](TERRAIN-WATER-GEOMETRY.md).

## Generator paths to material instances

The `Valleys` and `Mountains` `TerrainGenerator` resources both store these filenames, and
`FUN_1400b5700` copies them into the terrain setup object before generating the grid:

| Generator filename | Extracted resource | Resource type | Material reference |
|---|---|---|---|
| `Terrain\\Terrain.rsc` | `Terrain/Terrain_resource.crs` | `MaterialInstance` (`0x2ebeed54`) | `Material/Terrain/TerrainMaterial.rsc:resource` |
| `Terrain\\River.rsc` | `Terrain/River_resource.crs` | `MaterialInstance` (`0x2ebeed54`) | `Material/River/RiverMaterial.rsc:resource` |
| `Terrain\\Lake.rsc` | `Terrain/Lake_resource.crs` | `MaterialInstance` (`0x2ebeed54`) | `Material/Lake/LakeMaterial.rsc:resource` |

The mapping is supported by the paths in the serialized generator records, the matching extracted
files, and each file's serialized material reference. The MapComponent render method submits its
path-derived group list with the River material and its separate lake-point group list with the Lake
material; see [TERRAIN-WATER-GEOMETRY.md](TERRAIN-WATER-GEOMETRY.md).

## Texture slots

The three material instances serialize these named texture inputs:

| Instance | Slot | Referenced texture resource |
|---|---|---|
| Terrain | `grass` | `Terrain/TerrainGrassTexture.rsc:resource` |
| Terrain | `dirt` | `Terrain/TerrainDirtTexture.rsc:resource` |
| Terrain | `rock` | `Terrain/TerrainRockTexture.rsc:resource` |
| Terrain | `snow` | `Terrain/TerrainSnowTexture.rsc:resource` |
| River | `water` | `Terrain/RiverWaterTexture.rsc:resource` |
| River | `frame` | `Terrain/RiverFrameCopyTexture.rsc:resource` |
| River | `reflection` | `Terrain/RiverReflect2Texture.rsc:resource` |
| River | `highlight` | `Terrain/RiverReflectTexture.rsc:resource` |
| Lake | `water` | `Terrain/LakeWaterTexture.rsc:resource` |
| Lake | `frame` | `Terrain/RiverFrameCopyTexture.rsc:resource` |
| Lake | `reflection` | `Terrain/RiverReflect2Texture.rsc:resource` |
| Lake | `highlight` | `Terrain/RiverReflectTexture.rsc:resource` |

The Lake instance reuses the river frame-copy, reflection, and highlight texture resources. These
are serialized resource links and slot names; shader behavior and runtime binding have not yet
been traced.

## Next trace

- Identify the material-instance serializer and any conditional slot use before documenting
  rendering rules.
- Trace material texture slots through shader code to document the visual effects of each binding.
