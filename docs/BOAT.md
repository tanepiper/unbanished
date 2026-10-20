# Trader boat geometry

The M3 boat mismatch is resolved. The skeleton convention was correct; the reader discarded
the rigid boat geometry when it found the merchant's skinned vertex buffer in the same detail
level. Comparing the merchant alone with the complete boat's bounds produced the apparent
2.7-unit error.

## Evidence

The existing Ghidra 12.1.4 exports of `Runtime-steam-x64.dll` establish the draw-list semantics:

- `Graphics::MeshInstance::Draw` at `0x1801031f0` selects the 24-byte static stream and a node's
  world transform when the sixth triangle-list integer is nonnegative. A negative value selects
  the 32-byte weighted stream and the skin palette.
- `Graphics::MeshInstance::AddDrawItems` at `0x1801037b0` reads the fifth integer at triangle-list
  offset `0x10` as a material-list index. It is not a base vertex.
- `Graphics::GraphicsMesh::PlatformSerialize` at `0x1800a5830` stores both geometry streams per
  detail level. The first integer of each triangle list supplies the base vertex; the third
  supplies the starting index. Data confirms that the oar index ranges contain local indices
  `0..59` and require base vertices 1,282 and 1,342 respectively.

These are static observations from the existing Ghidra exports in the ignored
`analysis/export/Runtime-steam-x64.dll/decomp.c`, corroborated by the player's extracted
`Models/Buildings/TradingPost/BoatMesh_resource.crs`. No execution of the original game was
needed to establish these fields.

The boat's single detail level contains five triangle lists:

| Geometry | Stream | Base vertex | Start index | Index count | Material | Node |
|---|---|---:|---:|---:|---:|---:|
| Hull | Static | 0 | 0 | 4,026 | 0 | 0 |
| Left oar | Static | 1,282 | 4,026 | 144 | 0 | 1 |
| Right oar | Static | 1,342 | 4,170 | 144 | 0 | 4 |
| Water blocker | Static | 1,402 | 4,314 | 57 | 1 | 19 |
| Merchant | Weighted | 0 | 0 | 1,611 | 2 | -1 |

## Result

The decoder retains both streams, names the material field correctly, and exposes stream
selection per draw range. The viewer submits each range with its own material, index range,
base vertex and rigid transform. It displays the first detail level instead of overlapping
all detail levels. Weighted geometry continues to use `world * inverse_bind`.

For the combined boat in its bind pose, reconstructed and stored bounds are:

| | Minimum | Maximum |
|---|---|---|
| Stored | (-2.631563, -2.318575, -1.249396) | (2.056103, 2.309021, 0.048315) |
| Reconstructed | (-2.630859, -2.317793, -1.248047) | (2.056641, 2.309654, 0.050060) |

The greatest difference is 0.001745 units, consistent with the packed vertex precision. All
eight skinned meshes now pass `skinned_bind_pose_matches_bounds`; the boat exclusion is removed.
`decodes_all_supported_resources` also validates every draw range's local indices and base
vertex against its selected stream across all 271 meshes. `cargo check -p unbanished-viewer`
passes for the native target. A native Metal headless capture on Apple M3 Max was visually inspected:
hull, cargo, both oars and merchant appear together. The original water mask effect is not reproduced.

This resolves bind-pose geometry and its bounds comparison. Animation playback and original
material effects such as the water mask are outside this geometry finding.
