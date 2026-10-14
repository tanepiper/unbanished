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
The profile objects also name `Terrain/River.rsc` and `Terrain/Lake.rsc`, but the examined call chain
does not bind a specific record group to either resource. The mesh is therefore called
path-derived geometry here, without labeling it river or lake geometry.

## What remains unresolved

- Trace map-state offsets `+0xf8`, `+0x150`, and `+0x158` from the copied terrain, river, and lake
  filenames through resource loading and drawing.
- Identify the source/meaning of the packed per-cell channels and the encoded vertex normal bytes.
- Compare generated path meshes with captured runtime maps before assigning a visible feature name.
