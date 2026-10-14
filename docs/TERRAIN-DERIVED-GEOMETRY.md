# Terrain-derived geometry

This note follows generated terrain data into its render buffers. The evidence is static Ghidra
12.1.4 pseudocode for the Steam x64 executable; there is no runtime map capture in this trace. The
grid transforms and shipped profile values are in [TERRAIN-GRID-TRANSFORMS.md](TERRAIN-GRID-TRANSFORMS.md)
and [TERRAIN-PROFILES.md](TERRAIN-PROFILES.md).

## Height grid to tiled surface buffers

`FUN_1400b1980` converts the generated float grid to signed 16-bit entries by multiplying by a
constant and truncating to an integer. It then calls `FUN_1400b16b0` to create terrain surface
buffers. That function partitions the map into 32-by-32-cell tiles. Each cell contributes two
triangles, and the diagonal alternates according to `(cellX + cellY) & 1`. The matching vertex
builder, `FUN_1400b1d50`, reads the signed 16-bit grid sample for each vertex and stores it in a
16-byte vertex record with local coordinates, normal-related bytes, and texture coordinates.

`FUN_1400b1c50` separately uploads the packed per-cell data returned by `FUN_1400d4910` to an
image buffer sized to the terrain grid. Its pixel meaning is still unresolved; the previous note
records its threshold/noise-derived packed channels.

## Path records to a second mesh

After the terrain tile buffers are created, `FUN_1400b1980` calls `FUN_1400b0730` with the same
generation context that contains the counted 28-byte path records. For each record group,
`FUN_1400b0730` calls `FUN_1400d4fd0`, which expands/interpolates the path points and returns
geometry-related arrays. The caller builds 16-byte vertices from the returned point data, appends
indices, then creates a separate vertex buffer at map-state offset `+0x160` and index buffer at
`+0x180`.

This proves that the generated path records feed a second mesh alongside the tiled terrain surface.
The MapComponent render path now resolves its groups: records from the `+0x18` path collection are
submitted with the River material, while a separate `+0x48` point collection and height-grid pass
are submitted with the Lake material. The producer-to-render details are in
[TERRAIN-WATER-GEOMETRY.md](TERRAIN-WATER-GEOMETRY.md).

The three referenced `.rsc` files resolve to material instances and their serialized texture slots;
see [TERRAIN-MATERIAL-RESOURCES.md](TERRAIN-MATERIAL-RESOURCES.md). The renderer submits the
path-derived groups with those water materials as described above.

## What remains unresolved

- Trace material-instance slot use through the shader and drawing code to identify the visual
  terrain class and water rendering rules.
- Identify the source/meaning of the packed per-cell channels and the encoded vertex normal bytes.
- Compare generated geometry with captured runtime maps to verify the static producer-to-material
  mapping against in-game appearance.
