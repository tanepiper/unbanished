# Sun shadows

The original casts one orthographic shadow map from the sun each frame. The port follows it in
`apps/game/src/shadow.rs` (CPU fit and GPU resources), the depth pipelines in `models.rs` and
`render.rs`, and the shared `sun_shadow()` lookup in `scene_gfx::GLOBALS_WGSL`.

## Map and settings

| Item | Original | Port |
|---|---|---|
| Shadow map | `SunLight::SunLight` 0x18010aa80: a 2048² depth `ImageBuffer` (format 7) with flag 0x40 (`ImageBuffer::IsShadowMap` 0x18001f400) at `SunLight +0xf8`. The size is shifted right by `Video::Configuration +0x2c` ("ShadowScalar", default 1). | `shadow::shadow_size(ShadowScalar)`: 1024 by default, `Depth32Float`; the Options dialog's Shadow Resolution rebuilds the renderer |
| Shadow detail | `Configuration +0x30` ("ShaderDetail", default 2), copied to `Renderer +0x3c`. It selects the program variant of every lit shader: 0 none, 1 one tap, 2 four taps, 3 nine taps (`StringTable_shadowDetail`: None/Low/Medium/High). `DrawShadowMap` does nothing at 0. | `render::VideoSettings::shadow_detail` → `shadow_params.z`; `sun_shadow()` takes one, four or nine taps, and detail 0 skips the shadow pass |
| Taps | Program 2: four taps at (±½, ±½) texels, averaged; `pc[0].x` = 1 / map width (`Context::SetLight` 0x18001b230 stores it at `Context +0x248`). Program 3: 3×3 taps at ±1 texel / 9. | `sun_shadow()` |
| Lookup | Hardware depth compare (`ref <= stored` lit) with bilinear PCF. The lit term is `light · shadow · sat(−n·l) + ambient · …`. The river's constant 0.165 sun term (program 0) becomes the sum of the four taps × 0.04125. | Comparison sampler, LessEqual, linear, clamp |

## Sun camera (`SunLight::GetSunCamera` 0x1801098a0)

It reads the main camera from the renderer's first draw group (`Renderer +0x10`, entry `+8`):
frustum (`+0x08`, six planes and an ortho flag), rotation rows right/forward/up (`+0x6c`) and
position (`+0x90`).

1. `Volume::Frustum::GetCorners` 0x1800fc0e0 gives the near-plane corners (perspective planes
   from `Frustum::Frustum(near, far, hfov, vfov)` 0x1800fca70).
2. Each corner ray (normalized, rotated to world) meets the plane z = −2. A ray that hits it within
   1000 contributes the hit point twice. Otherwise it contributes the corner and the point 1000
   further along the ray. That point's z is replaced by float `2 + 3i` of the flat point array,
   clamped to −2..15. The original indexes its eight-point array with the corner stride, so
   corner i reads a coordinate of an earlier point. The port keeps this.
3. The centroid of the eight points, moved 128 back along the sun direction, is the camera
   position. The frustum half-width is the largest centroid distance × `sqrt(|forward.z|)`.
4. The axes are `Math::Transform3::FromLookAt(0, sunDirection)` 0x180065220 (forward = the
   direction the light travels, right = forward × z, up = right × forward). The position is
   snapped per axis with `v − fmod(v, 2·radius / size)`.
5. The frustum is orthographic, ±radius, near 1, far 256 (`Camera::Camera` 0x1801011f0 →
   `Camera::SetFrustum` 0x180100e50 → `Frustum::Projection` 0x1800fc650, D3D form: depth
   `(y − 1) / 255`).

**Verified bit-exact:** `tools/emu/sun_camera.py` runs the original function under Unicorn over
400 random main cameras (including views whose rays miss the ground) and sun directions. The
test `shadow::tests::matches_the_original_get_sun_camera` reproduces all 400 exactly.

## Projection (`SunLight::Draw` 0x180109040)

`lc[10..13]` = inverse(sun transform) · projection · bias, where the bias (for D3D, Device
virtual +0x38) is `u = x/2 + ½`, `v = −y/2 + ½`, z unchanged. The vertex shaders write
`world · lc[10..13]` to a texture coordinate (`t4` in TerrainMaterial_Normal_vs).
`Context::SetShadowProjection` 0x18001b500 writes the same slot (`Context +0x1e78`).

## Depth pass (`SunLight::DrawShadowMap` 0x180109450)

- `SunLight::AddDrawItems` 0x1801097e0 opens a draw group with the sun camera as draw camera,
  the main camera as **cull camera**, pass 1 (each material's `depth` pass) and material-type
  mask 1 (bucket 0: Terrain, Opaque*, AlphaTest, Billboard*, Foliage, Opaque Skin/Character,
  Sky). The Sky's depth pass has no programs, so it is skipped.
- `DrawShadowMap` clears depth to 1 and sets the viewport to (1, 1, size − 2, size − 2), which
  leaves a lit one-texel border. The lookup still assumes the full map, a scale of
  (size − 2) / size that the port keeps.
- D3D9 rasterizes at integer pixel centres, so each texel stores the depth half a texel up-left
  of where it is sampled. The port moves the depth pass's geometry by that half pixel.
- **Culling.** The `rsDepth` render states set the culling. `RenderState::Serialize`
  0x1800f74e0 gives the field order. The int at `+0x68` maps to D3DRS_CULLMODE through
  {CW, CCW, NONE} (VideoDX9 0x180003380 table).

  | State | Cull | Bias / slope bias | Used by |
  |---|---|---|---|
  | `OpaqueMaterial_rs` | CCW (keep front) | 0 / 0 | colour passes |
  | `OpaqueMaterial_rsDepth` | **CW** (keep back) | 0 / 0 | Opaque, Snow, InWater, Skin, Character, **Terrain** |
  | `OpaqueMaterial_rsRef` | CW | 0 / 0 | mirrored reflection (consistent with the reading) |
  | `AlphaTestMaterial_rsDepth` | CCW | 1e-4 / 1e-4 | AlphaTest, Foliage |
  | `BillboardMaterial_rsDepth` | none | 0 / 0 | Billboard |

  Opaque casters therefore store their back faces. The single-sided terrain never reaches the
  map, and sun-facing surfaces cannot shadow themselves. Port: `shadow::DepthState`.
- Depth pixel shaders: `Depth_ps` has no alpha test; `DepthAlpha_ps` (AlphaTest, Foliage) cuts
  alpha at **0.25**, half the colour pass's 0.5; `Billboard Depth_ps` has the colour pass's leaf
  thinning and 0.5 cut (`Depth_psevergreen` without thinning). Vertex programs place vertices as
  the colour passes do. Billboards face the sun camera because the pass's camera vectors are the
  sun's.

## Comparison with the original

Captured at 3456×2168 and compared with `imgdiff --resize --blur 3 --threshold 32`:

| View | No shadows | Shadows |
|---|---|---|
| Main menu scene 6 (`ui/main-menu`) | MAE 22.3, PSNR 16.9 dB | MAE 19.8, PSNR 17.8 dB |
| Seeded start (`seed-999518904/start-t0`, compare.sh) | MAE 17.1, PSNR 21.1 dB (earlier report) | MAE 15.7, PSNR 21.7 dB |

With culling as in `rsDepth`, the menu's pasture matches the original's brightness (red mean
0.417 in the port vs 0.40 in the original). When the terrain was drawn into the map in a test,
acne dimmed the pasture to 0.29.

## Not ported / deviations

- The shadow detail and resolution options are fixed at their defaults.
- Casters are not culled against the main camera's frustum.
- The reflection pass reuses the main view's shadow map.
- Evergreen billboards share the fading depth shader, matching the port's colour pass.
