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

`FUN_1400b1c50` separately uploads the packed per-cell data returned by `FUN_1400d4910` to a
`size × size` image buffer in format 5 (BGRA8). The terrain pixel shader samples it as
`transitionMap`, which fixes the channel meanings (see below).

### Tile vertex format (verified)

`FUN_1400b1d50` writes 16-byte vertices; the Rust port (`crates/terrain/src/mesh.rs`) is
bit-exact against the emulated original for every tile of a 512-cell map
(`tools/emu/terrain_mesh.py`).

| Bytes | Content |
|---|---|
| `i16 x, y` | cell coordinate minus `size / 2` |
| `i16 z` | saved height (`grid × 256`), clamped sample |
| `i16 w` | `0xff` |
| `i16 u, v` | `(float)cell / size × 16384`, truncated |
| `u8 x, y, z` | `(n + 1) × 127.5` of `normalize(-Δx / 256, -Δy / 256, 2)` (`FUN_1400b3740`, central integer differences, edge-clamped) |
| `u8` | 0 |

Tiles are 32×32 cells with 33×33 vertices (MapComponent `+0xc8`, `+0xcc`, `+0xd4 = 1089`). The
shared index list emits per cell `a, c, b, b, c, c+1` when `(x + y)` is even and
`a, c+1, b, a, c, c+1` when odd (`a` = top-left, `b = a + 1`, `c = a + 33`).

### Surface image `FUN_1400d4910` (verified)

The port in `crates/terrain/src/surface.rs` is bit-exact against the emulated original on
four 512-cell maps. It consumes two `RandTol(0xffffff)` values after generation to seed a
`PerlinNoise(0.25, 4, second, first)`. Per cell, with `h` the float height and `up` the z of
`normalize(-(h[x+1]-h[x-1]), -(h[y+1]-h[y-1]), 2)` (`FUN_1400d4620`):

| Byte | Shader use | Value |
|---|---|---|
| 0 (`.z`) | lerp grass → dirt | `h < -0.5` → 255, else `(noise(x/4, y/4)·0.5 + 0.5)² · 255`; then max with `clamp((h-8)/3)·255` when `h > 8` |
| 1 (`.y`) | lerp → triplanar rock | `up ≥ 0.999` → 0, `up ≤ 0.85` → 255, else `(1 - (up - 0.85)/0.149)·255`; then max with `clamp((h-7)/3)·255` when `h > 7` |
| 2 (`.x`) | ambient multiplier | `(1 - mean(max(0, n·dir)))·smooth_up·255` over neighbours with `dx² + dy² ≤ 100`, `dy ∈ [-10, 9]`, `dx ∈ [-10, 10]` (the row range really is asymmetric); `smooth_up` is the z of the normalised sum of the 3×3 normals |
| 3 (`.w`) | snow (max with `up⁴ · c4.x`) | 0 |

Byte conversions are `cvttss2si` to 64 bits keeping the low byte.

### Terrain shaders

`tools/shader/d3d9_disasm.py` disassembles the D3D9 programs inside `PixelProgram` /
`VertexProgram` resources (each also carries D3D11 DXBC). `TerrainMaterial_vs` program 0:
the normal is `nrm(v2·2 - 1)`; world position is `tc_transform · (x, y, z/256)`; outputs the
detail UV `xy/16`, the surface UV `uv/16384`, the rock triplanar coordinate `z/4096` and an
underwater factor `-(z/256 + 0.75)/1.5`, plus `-n·lc_nearPlane` (diffuse) and `(n·L + 3)/2`
(ambient factor). `TerrainMaterial_ps` program 0 samples grass `s0`, dirt `s1`, snow `s2`, rock
`s3` and `transitionMap` `s6`; rock is triplanar (`|n|`-weighted samples at `(x, z/4096)`,
`(y, z/4096)` and `(x, y)`), the surface lerps grass→dirt→rock→snow, tints towards
`(0.025, 0.123, 0.19)` underwater, lights with `lightColor·sat(diffuse) + ambientColor·ambient·
transition.x`, applies fog, and writes the underwater factor to alpha.

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
- Compare generated geometry with captured runtime maps to verify the static producer-to-material
  mapping against in-game appearance.
