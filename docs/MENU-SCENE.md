# Startup, main menu and the menu scenes

This note covers how the executable boots to the main menu and what the 3D menu background is. The
evidence is Ghidra 12.1.4 pseudocode and disassembly of the Steam x64 build plus decoded shipped
resources. The implementation is `apps/game` (`loops.rs`, `menu.rs`, `models.rs`) and
`crates/formats/src/menu_scene.rs`. All ten scenes parse exactly
(`crates/formats/tests/menu_scenes.rs`).

## Startup chain (`FUN_140042850`)

After `Engine::Engine::Startup`, the game builds a chain of loader loops. Each one shows something
while the next loads on a background thread:

1. A `LoaderLoop` named "Game/Global" (`FUN_14009f280`). It loads the New Game lists and UI sounds.
2. A `DialogLoaderLoopT("Dialog/ShiningRock.rsc")`, the splash, shown for at least 2.0 s.
3. A `DialogLoaderLoopT("Dialog/Loading.rsc")`, shown for at least 0.5 s. It has a spinner, and
   every 2 s `descText` shows `StringTable:loadingDetails[Rand(count - 1) + 1]`.
4. The menu loop (vtable `0x140165120`, resource group "Game/Menu").

The splash and loading loops fade in from black when they start (`FUN_14009e470`). Once loading
is done they fade out and exit (`FUN_14009e270`). `UI::UserInterface::UpdateFade` moves the fade
at 4 units per second (0.25 s). `DialogLoaderLoopT::Draw` sets `pc_userPosition.x` to the
seconds since the loop started.

The splash sprite uses `Material\UI\Logo`. Its pixel shader treats the 1600×900 texture as three
layers:

- red: the logo;
- green: where rays may appear;
- blue: a ray pattern, sampled after rotating by `userPosition.x` radians about texel (800, 421).

It outputs `1 − (r + g·(1 − b′)) × (0.83, 0.79, 0.75)`, a dark logo with turning rays on white.

New and loaded games use one of twelve loading screens, `Dialog/Loading.rsc:gameLoad%i` chosen at
random (`FUN_14009e600`).

## Menu loop

- **Load (`FUN_14009fe00`).** It finds `Audio/Music/Menu.rsc`, a looping 199 s stereo track. It
  picks `Game/MenuScene/Scene{Rand(10)}.rsc` with a `System::Random()` seeded from the clock. If
  the scene fails to load, the menu has no 3D background.
- **Startup (`FUN_1400a0e00`).** It fades in and plays the music on audio category 2.
- **Dialogs (`FUN_1400b91a0`).** `MainMenuDialog` binds the button IDs listed in
  [NEW-GAME-FLOW.md](NEW-GAME-FLOW.md) and adds `Dialog/BuildId.rsc`. The build notice parameters
  are `Configuration::CurrentVersion()` ("1.0.7") and `CurrentBuildId()`, which is the
  `__DATE__` "Sep 10 2017" formatted as `yymmdd` ("170910").
- **Update (`FUN_1400a0bb0`).** It sways the camera: `angle += dt · 11.25`, then
  `yaw = base + 2 · cos(angle°)`.
- **Draw (`FUN_1400a0910`).** The camera frustum is set by `FUN_14009db30(40, 2, 512)`: 40°
  vertically and `aspect × 40°` horizontally up to 16:9, with each angle used directly. The draw
  sets the sun (`FromYawPitch(yaw, pitch)`), the light and ambient colours, the fog and
  `pc_userPosition`, renders the 3D scene to an offscreen target, copies it to the window, and then
  draws the UI and fades over it.

## Current menu-scene rendering

The headless renderer has been captured for menu scenes 0, 1, 3, 5 and 9. The current scene pass
draws the sky, terrain, placed models and ground decals offscreen; if water is present, it also
renders a half-size planar reflection, copies the scene for refraction, and draws the river/lake
surface. The final offscreen scene is blitted to the window before UI composition, matching the
original render order.

The implemented scene details are:

- **Sky:** the sky dome samples its cube texture and mixes toward the mirrored overcast view using
  the scene's snow and rain values.
- **Water:** rivers and lakes use moving ripples, the copied scene for refraction, the planar scene
  reflection, environment highlights and soft shoreline alpha.
- **Ground markings:** tiled decals draw roads, market squares, fields and building footprints,
  including snow coverage where the decal material supports it.
- **Snow:** terrain, roof materials and pines use each scene's `userPosition.x` snow value.
- **Pines:** foliage vertices sway using their wind weights and scene time.
- **Deciduous trees:** billboard leaf cards turn with autumn colour and thin out with leaf loss.

The scene fields are the values from the original MenuScene resources; these captures establish
that the listed paths render in the headless build for those five selected scenes. They do not
verify every one of the ten scenes in a browser or against a live game capture.

## MenuScene format (`FUN_1400a8b00`, version 2)

All ten files hold the same Medium town (768-cell terrain), seen from different camera positions,
times of day and seasons.

| Field | Content |
|---|---|
| name | source `.msc` path |
| MapComponent | `FUN_1400b59a0` (v6), then `FUN_1400b4670` (v4), described below |
| camera | `Transform` (quaternion, position), target `Vector3`, yaw°, pitch°, distance |
| placements | `{x, y, z, sx, sy, sz, yaw°, i16 mesh, i16 material set}` |
| meshes | external references (`GraphicsMesh`) |
| sky | external reference (`Models\Sky\SkyMesh.rsc`) |
| userPosition | `Vector4`: snow, autumn colour, leaf loss, rain |
| colours | ambient, sun, fog (`0xAARRGGBB`) |
| floats | fog start, fog end, fog maximum, sun yaw, sun pitch |
| v > 1 | weather effect reference (`Environment\Rain.rsc`) and intensity, particle placements `{x, y, z, yaw, system}`, particle system references (chimney and iron smoke) |

The camera eye is `target − FromYawPitch(yaw, pitch) × distance`. This matches the stored
transform position. A placement's transform is `FromYawPitchOffset(yaw, offset)` with its scale,
which the model shaders read from the `w` of the transform rows.

## MapComponent block (`FUN_1400b59a0`, version 6)

Saves use the same block, so loaded games can be drawn the same way. Its fields:

- the six dimensions (`2n`, `n`, 32, 33, `2n+1`, 33²);
- the `i16` heights;
- the terrain material;
- the `size²` packed surface image;
- the water mesh: 16-byte vertices `{i16 x, y, z, w; i16 u, v; u8 ×4}` and `u16` indices, with
  x and y in 1/48 cell and z / 256;
- the River and Lake materials, and draw ranges per river and per lake;
- the river centre lines;
- tiled decals (`FUN_140020900`): material, five ints, a byte, an int, a bool, five floats and a
  bool, then in v > 1 another bool, then 20-byte vertices and indices;
- in v > 3 an int list, and in v > 5 the decal materials.

`FUN_1400b4670` serializes the decal lists again.

## Model materials in the scenes

The disassembled D3D9 programs give the shading:

- **OpaqueMaterial:** `diffuse × (sun × sat(−n·L) + ambient × (n·L/2 + 1.5) × AO)`, then fog.
- **AlphaTest** (herbs, crops) **and Foliage** (pines): the same with a 0.5 alpha cut. Foliage
  also sways by the per-vertex wind weight and scene time.
- **BillboardMaterial** (deciduous trees). A vertex with a non-zero `userData` corner is a leaf
  card:
  - it is pushed out along the camera's right and up axes, bent towards the view by the normal;
  - it sways with `0.025 · sin(world.xy + time)`;
  - it is lit with a crown normal `normalize(local.xy, local.z + 4)`;
  - in the pixel shader, leaves drop with `userPosition.z` (alpha) and turn autumn-coloured with
    `.y`, using `lum × (0.914, 0.725, 0.278)`.

The other materials in use are OpaqueSnow (snow on roofs from `userPosition.x`), OpaqueInWater
and their Damage variants. OpaqueInWater and Damage variants currently use the Opaque draw path.

## Remaining menu-scene work

The renderer still does not draw chimney or forge smoke, or falling rain. The menu town-name box is
empty; the generator path has been traced but its name-list file still needs decoding. Menu music
currently decodes the full track at once, which can pause the loading spinner; stream or chunk the
decode to keep loading responsive. Button click sounds are not wired. Browser playback start-after-
first-click behavior has not been confirmed in a real browser. A small visual issue remains: distant
summer trees look too pale. These shader and interaction details should be checked against saved
headless captures and, where relevant, a real browser session.
