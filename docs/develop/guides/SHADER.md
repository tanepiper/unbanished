# 5. Port a shader

The original draws with Direct3D 9 shaders. They ship compiled, as `VertexProgram` and
`PixelProgram` resources. Un-Banished draws with wgpu, so each shader is translated to WGSL by
hand, with the same maths. The result must also run on WebGL 2 in the browser.

## 1. Find and disassemble the shader

A material refers to its programs by resource path. Decode the material to find them
([Decode a format](FORMAT.md)). Then disassemble the Direct3D 9 bytecode:

```bash
python3 tools/shader/d3d9_disasm.py game-data/extracted/<path>/X_vs.crs          # list the variants
python3 tools/shader/d3d9_disasm.py game-data/extracted/<path>/X_ps.crs 0 > analysis/X_ps0.txt
```

Each resource holds several Direct3D 9 variants, then DXBC blobs. The constant table (CTAB)
names each constant register, for example `view_proj`, `light_dir` or `fog`. Choose the variant
that the game uses; it is usually 0, and the material's program index confirms it. Keep the
output in `analysis/`. Never commit it.

## 2. Translate

- Translate one instruction at a time first. Simplify only where the result is provably
  identical.
- Keep the original constants exactly, for example `0.00390625` and `-0.666666687`.
- Watch the differences between Direct3D 9 and wgpu:
  - Clip-space depth is 0 to 1 in both. But Direct3D 9 matrices are row vectors (`m4x4` is a
    dot product with each row). Check the transpose against the CPU code.
  - `texkill` becomes `discard`.
  - `lrp(a, b, c)` becomes `mix(c, b, a)`.
  - `sge` and `slt` become `select` or `step`.
  - The `_sat` modifier becomes `clamp(x, 0.0, 1.0)`.
- World space is the terrain's: x and y in cells, centred on the map; z is up and equals the
  saved height divided by 256.
- Put the shared `GLOBALS_WGSL` (in `scene_gfx.rs`) at the start, instead of declaring `Globals`
  again. Call `reflect_clip(world_z)` for the reflection pass of the water.

## 3. Rules for WGSL and WebGL 2

These rules were learned from real failures:

- `textureSample` needs uniform control flow. **Do every texture sample before any
  `discard`.** Put `reflect_clip()` and alpha-test discards at the end.
- WebGL 2 has no base-vertex draws, limited storage buffers in vertex shaders, and needs the
  `downlevel_webgl2_defaults` limits. Use vertex buffers, index buffers and uniforms.
- Render targets use `SCENE_FORMAT` (`Rgba8Unorm`) and `DEPTH_FORMAT` from `scene_gfx.rs`.
- To keep the destination alpha (for example, decals over the terrain), use the alpha blend
  `Zero`/`One`.

## 4. Verify

Render a headless capture ([Verify](VERIFY.md)) and compare it with a screenshot of the original
game from a similar view. Write any intentional difference in the shader's header comment.
Record the mapping of the shader's constants in the topic document, for example
[TERRAIN-DERIVED-GEOMETRY.md](../../TERRAIN-DERIVED-GEOMETRY.md).
