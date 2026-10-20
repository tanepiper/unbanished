# Map window (MapUI) and the minimap

Slice G covers the Map utility window (`Template/UtilityMap.rsc`, host
`Dialog/StandardDialog.rsc`, element `Dialog/Map.rsc:content`) and the minimap images it shows.
Port files:

- `apps/game/src/ui/utility.rs`: `MapUi`.
- `apps/game/src/gfx/render.rs`: `update_minimap` and `render_minimap`.
- `crates/sim/src/town/minimap.rs`: `minimap_overlay`.
- `crates/ui`: the `Lines` kind.
- `crates/formats`: `FrustumOutline`.

Evidence: Ghidra pseudocode of the Steam x64 executable and Runtime DLL.

## MapUI (`0x42c5425e`, vtable `0x140162490`)

- **OnOpened `FUN_14006d3e0`.** Wraps the minimap's two images (`Game +0x148`: `+0x8` terrain,
  `+0x10` overlay) as one-sprite sheets (`FUN_14006d460`). They are shown in `imageMap` and
  `imageOverlay`, both 192×192. The overlay gets flag `0x10000` and element id 1000.
- **Event `FUN_14006d340`.** Kind 2 on element 1000 calls `FUN_14006d1b0`. A press at pixel
  `(x, y)` of a W×H overlay sets the camera pivot to `((x - W/2)·n/W, (H/2 - y)·n/W, 0)`, where
  `n` is the map size. The port does this on a left click over the overlay.
- **No refresh.** The `frustum` element draws itself.

## FrustumOutlineElement (`FrustumOutlineDescription`, a game class)

The description has the same block as an ImageDescription: sheet
`Dialog\SpriteSheetLine.rsc:resource`, sprite `White`, size 192×192. The element colour is
`0x80ffff00`. Draw `FUN_14006c7f0`:

1. It calls `Element::Draw`, then `FUN_14006cf10`. That function takes the camera frustum's
   four far corners, rotates the normalized rays by the camera, and meets them with the
   ground plane `z = 0`.
2. It maps each corner to element pixels with `x·W/n + W/2` and `(W - 1) - (y·W/n + H/2)`.
3. It draws `SpriteContext::AddLine` for 0-1, 1-3, 3-2 and 2-0, scissored to the element.

The UI crate's `Lines` kind draws the segments as one-pixel sprite quads.

## Minimap (`Game +0x148`)

| Offset | Field |
|---|---|
| `+0x0` | overlay dirty |
| `+0x4` | size `n` (the map size) |
| `+0x8`, `+0x10` | textures |
| `+0x18` | terrain pixels |
| `+0x28` | overlay pixels (`n²` u32, zeroed by `FUN_1400aa550`) |

- **Terrain image (`FUN_1400aa610`).** It is rendered once, from the game draw `FUN_14009b4e0`
  when the texture does not exist yet. The scene is the terrain and water geometries
  (`Game +0x110`, `+0x100`).
  - Camera: orthographic, ±n/2, near 0, far 512, at height 100, yaw 0, pitch -90.
  - Fog: 1000 to 1001, white. Light vectors `(0, 0, 1)`.
  - Output: an `n×n` render read back into `+0x18`.
  - The renderer is a fresh `Graphics::Renderer` with no reflection pass. The port therefore
    gives the water an opaque black reflection, so rivers and lakes show their beds darkened
    (inferred from the reference's dark water). Before this the magenta clear showed through.
  - The port renders terrain, reflection and water with its own pipelines and blits into an
    `n×n` texture, `Texture::External(1)`.
  - The two `(0, 0, 1)` context vectors are `Context::SetUserPosition((0, 0, 1, 0))`:
    `pc_userPosition`, so the minimap has no snow, no autumn colour and no rain.
  - Its scene has no light object, so the light is whatever the shader constants hold. It runs
    before the frame's `Renderer::Draw` (`FUN_14009b4e0`). Only `SunLight::Draw` and
    `SpotLight::Draw` call `Context::SetLight` (`AmbientLight::Draw` is empty, and no light
    resources ship), and `Context::Reset` clears only the light block at `+0x1dd8`, not the
    constants `SetLight` copied to `+0x268`. So the minimap is lit by the previous 3D frame's
    sun: the main menu scene's when a game starts from the menu, the old game's when loading
    from inside one. The port keeps the last scene's sun direction, colour and ambient for it.
- **Overlay (`FUN_1400aa200`).** A MapComponent whose description has `+0x32` set fills its
  map rectangle with the description colour (`+0x34`) when activated (`FUN_140129680`). Rows
  are flipped (`row = n - y - 1`) and the dirty flag is set; `FUN_1400aa5b0` uploads it.
  - The port recomputes the overlay from the live placed entities whenever models change and
    uploads it as `Texture::External(2)`.
  - Removal (`FUN_1401297c0`) is assumed to clear the rectangle. Recomputing gives that result.

## Verification

- Capture: `UNBANISHED_SEED=999518904 unbanished --boot-capture OUT 12 --capture-size 1384x868
  --press buttonNew@4.5 --combo comboTerrain=Mountains@4.8 --combo comboTerrainSize=Small@4.8
  --press buttonOk@5.0 --script "open Template/UtilityMap.rsc@10"`.
- Compared with `game-data/reference/seed-999518904/start-t0.png`:
  - Matches: window at the top left; north at the top; river from the north into the
    central lake; the eastern lake; the yellow frustum trapezoid.
  - Differences: the original's frustum is larger because its start camera sits higher. The
    original's terrain colours are lighter and greener.
- `tools/triage/determinism.sh 22 1234` stays identical. The minimap takes no Random.

## Pending

- Dragging on the overlay (kind 2 repeats while held).
- Which shipped MapDescriptions set the minimap flag. No start-town entity shows on the overlay
  yet.
