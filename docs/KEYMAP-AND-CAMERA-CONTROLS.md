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
| `0x21`–`0x22` | `ScreenshotNoUI`, `Screenshot` | keyboard | `0x4a`, `0x4b` | no |
| `0x23`–`0x2c` | `Tool0`–`Tool9` | keyboard | `0x0e`–`0x16`, `0x0d` | no |
| `0x2d`–`0x31` | `Camera0`–`Camera4` | keyboard | `0x30`, `0x2e`, `0x19`, `0x2c`, `0x18` | camera presets |

Mouse actions are separately registered as `MouseX`, `MouseY`, `MouseDeltaX`, `MouseDeltaY`,
`MouseWheel`, `MouseLeft`, `MouseRight` and `MouseMiddle` at IDs `0x00`–`0x07`.

`FUN_140104390` reads the named action IDs for rotation, movement, zoom, pitch and mouse-driven
camera movement, then writes the resulting camera transform. `FUN_140104bb0` reads `TogglePause`
and the five `Camera0`–`Camera4` preset actions. `FUN_140103da0` applies a selected preset from the
controller's preset table. These reads corroborate the label-to-ID mapping independently of the
registration call.

## Relevance to the port

The current port provides drag rotation, wheel zoom and WASD/arrows, which are a simplified control
scheme rather than a reproduction of these action bindings. The registration routine's final
`KeyMap::Load` means the game can use configured bindings; a faithful port should first decode the
serialized key-map settings and only then choose its default physical keys.

## Evidence pointers

- `analysis/export/Application-steam-x64.exe/decomp.c`, `FUN_14009e7c0` at `0x14009e7c0`.
- Same export, `FUN_140104390` at `0x140104390`, `FUN_140104bb0` at `0x140104bb0`, and
  `FUN_140103da0` at `0x140103da0`.
- `docs/GAME-LOOP-STEP-02.md` describes their place in the gameplay loop.
