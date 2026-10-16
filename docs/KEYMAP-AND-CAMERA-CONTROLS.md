# Key-map IDs and the gameplay camera

This note decodes the action IDs used by the `Game/Game` input path from Ghidra's
`FUN_14009e7c0` registration routine and confirms which actions its camera updater consumes.
Evidence is static pseudocode for the Steam x64 executable. The routine registers default device
and input-code pairs and then calls `Input::KeyMap::Load`; user settings can therefore override the
defaults below. The numeric code column is preserved as registered and is not translated into
physical key names here.

## Registered input actions

| Action ID | Registered name | Device | Input code | Used by camera trace |
|---:|---|---:|---:|---|
| `0x08` | `Cancel` | keyboard | `0x00` | — |
| `0x09` | `RotateLeft` | keyboard | `0x27` | yes |
| `0x0a` | `RotateRight` | keyboard | `0x1b` | yes |
| `0x0b` | `Forward` | keyboard | `0x2d` | yes |
| `0x0c` | `Back` | keyboard | `0x29` | yes |
| `0x0d` | `Left` | keyboard | `0x17` | yes |
| `0x0e` | `Right` | keyboard | `0x1a` | yes |
| `0x0f` | `ZoomIn` | keyboard | `0x4d` | yes |
| `0x10` | `ZoomOut` | keyboard | `0x50` | yes |
| `0x11` | `PitchUp` | keyboard | `0x4f` | yes |
| `0x12` | `PitchDown` | keyboard | `0x52` | yes |
| `0x13`–`0x16` | `StructureLeft`, `StructureRight`, `StructureType`, `PathMethod` | keyboard | individually registered | no |
| `0x17` | `TogglePause` | keyboard | `0x47` | pause toggle |
| `0x18`–`0x20` | `Toolbar0`–`Toolbar8` | keyboard | `0x01`–`0x09` | no |
| `0x21`–`0x22` | `ScreenshotNoUI`, `Screenshot` | keyboard | `0x4a`, `0x4b` | `screenshot.rs` (`FUN_14009b270`) |
| `0x23`–`0x2c` | `Tool0`–`Tool9` | keyboard | `0x0e`–`0x16`, `0x0d` | no |
| `0x2d`–`0x31` | `Camera0`–`Camera4` | keyboard | `0x30`, `0x2e`, `0x19`, `0x2c`, `0x18` | camera presets |

Mouse actions are separately registered as `MouseX`, `MouseY`, `MouseDeltaX`, `MouseDeltaY`,
`MouseWheel`, `MouseLeft`, `MouseRight` and `MouseMiddle` at IDs `0x00`–`0x07`.

`FUN_140104390` reads the named action IDs for rotation, movement, zoom, pitch and mouse-driven
camera movement, then writes the resulting camera transform. `FUN_140104bb0` reads `TogglePause`
and the five `Camera0`–`Camera4` preset actions. `FUN_140103da0` applies a selected preset from the
controller's preset table. These reads corroborate the label-to-ID mapping independently of the
registration call.

## Port

`apps/game/src/keymap.rs` registers the same 50 actions with these defaults and stores them like
`KeyMap::Save` (`Input<Action>` / `Input<Action>Alt` = `device << 8 | input`, −1 unbound). The
keyboard codes follow `StringTable.rsc:keyNames` (`Keyboard0` Escape … `Keyboard103` Numpad 9), so
code `0x2d` is W, `0x29` S, `0x17` A, `0x1a` D, `0x27` Q and `0x1b` E. The Options dialog's Input
tab rebinds them (see [OPTIONS.md](OPTIONS.md)).

The gameplay camera follows `FUN_140104390`:

| Input | Effect |
|---|---|
| RotateLeft / RotateRight held | yaw −/+ 90° per second |
| MouseMiddle held | yaw −= mouse Δx · 0.2 |
| Forward / Back / Left / Right held, or the pointer within 4 pixels of the left/top edge or 5 of the right/bottom (game option EdgeScroll) | the pivot moves `f·70 + (1−f)·12` units per second along the camera's ground axes, `f = (zoom target − 20) / 110`; keys override the edges |
| ZoomIn / ZoomOut pressed, mouse wheel | zoom target ∓5 within 20..130 |
| PitchUp / PitchDown pressed | pitch slider ±0.125 within 0..1 |
| TogglePause | pause toggle |
| Toolbar0–8, Tool0–9, Camera0–4 | the shown toolbar button holding that key (`FUN_1400bede0` hands them out by depth) |

The engine's mouse inputs 4, 5 and 6 are the left, right and middle buttons
(`Device::PlatformGetMouseState`, Runtime `0x1800e4660`: `VK_LBUTTON`, `VK_RBUTTON`,
`VK_MBUTTON`). While the button bound to `MouseMiddle` (action 7; the middle button) is held the
camera turns, `yaw −= mouse Δx · 0.2` (`FUN_140104390`; no pitch, no pan by mouse). A press of
`MouseRight` (action 6; the right button) lets the tool cancel what it is doing (a drag), else
ends the tool (`FUN_140104bb0`). The left button never turns the camera. The port reads both
through the key map, so rebinding them in Options moves them. `PathMethod` held turns the road tool's Manhattan
heuristic off (`FUN_14010ab60`, RoadTool `+0x40`). The screenshot keys write
`Screenshots\Screenshot%i.bmp` in the save directory, the frame without or with the UI
(`FUN_14009b270`, drawn by `FUN_14009b4e0`).

## Evidence pointers

- `analysis/export/Application-steam-x64.exe/decomp.c`, `FUN_14009e7c0` at `0x14009e7c0`.
- Same export, `FUN_140104390` at `0x140104390`, `FUN_140104bb0` at `0x140104bb0`, and
  `FUN_140103da0` at `0x140103da0`.
- `docs/GAME-LOOP-STEP-02.md` describes their place in the gameplay loop.
