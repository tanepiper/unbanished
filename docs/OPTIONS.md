# Options dialog and settings

The Options dialog (`Dialog/Options.rsc`, controller vtable `0x1401632e0`) opens from the main
menu and the pause menu (`MainMenuDialog` element id 4 → `FUN_1400b8ee0`). The port is
`apps/game/src/ui/options.rs`. Settings are in `settings.rs` and the key map in `keymap.rs`.

## Stored settings

The original keeps its settings in the registry, one value per `"%s%s"` (section + key). The port
uses the same names in a `KEY=VALUE` text file. Natively it is `banished-wasm/settings.txt` under
`%APPDATA%`, `~/Library/Application Support` or `$XDG_CONFIG_HOME`; `UNBANISHED_SETTINGS` overrides
the path. In the browser it is `localStorage["banished-wasm.settings"]`. Booleans are `true` or
`false`.

| Section | Keys (defaults) |
|---|---|
| `Video` | `Interface` (1), `Adapter` (0), `Width`/`Height` (−1: desktop), `WindowWidth`/`WindowHeight`/`WindowX`/`WindowY`, `RefreshN`/`RefreshD` (60/1), `Multisample` (1), `Fullscreen` (true), `VSync` (true), `TextureFilter` (0), `ShadowScalar` (1), `ShaderDetail` (2), `ReflectionDetail` (1), `Settings` |
| `Audio` | `UIVolume` (100), `MusicVolume` (50), `AmbientVolume` (100), `EffectVolume` (100) |
| `Input` | `MouseClipping`, `Legacy`, `MouseDisplay` (all false); `Input<Action>[Alt]` key bindings |
| `Game` | `UIScale` (100), `AutosaveInterval` (1), `StatusIconOpacity` (100), `StatusIconEnable` (true), `StatusIconScale` (100), `ShowCelcius` (false), `EdgeScroll`, `ShowWeather`, `AutoSlow`, `AutoPause` (true), `LastOption`, `LastTerrain`, `LastMapSize`, `LastClimate` (1), `LastStart` (1), `LastDisaster` |

## Dialog (`FUN_1400797a0` bind, `FUN_1400783f0` events)

| Element | Id | Behaviour |
|---|---:|---|
| `buttonOk` | 10 | Apply, then close (`FUN_140077e00`) |
| `buttonCancel` | 11 | Re-read input, key map, audio and game settings, keep the shown tab, close (`FUN_140077d10`). Escape (action 8) |
| `buttonApply` | 12 | Video `FUN_1400772e0`, input `FUN_140077830`, audio, game `FUN_1400778f0`, `WriteSettings` |
| `sliderUI`/`Music`/`Ambient`/`Effect` | 0–3 | 0..100; the volume changes while the slider moves (event 0x17) |
| `comboRenderer` | 4 | Interfaces; refills adapters, modes, antialiasing and filters |
| `comboAdapter` | 5 | Adapters; refills antialiasing, filters, modes |
| `comboResolution` | 6 | `%ix%i` modes; refills refresh rates |
| `comboRefresh` | 7 | Refresh rates as whole hertz (`n/d`) |
| `comboAntialias` | 8 | "None" (0), then "%ix MSAA" (data n for n + 1 samples) per supported count |
| `comboFilter` | 9 | Bilinear (−1), Trilinear (0), Anisotropic 1x (1) when supported, then 2x, 4x, … up to the maximum. Selection: −1 → 0, 0 → 1, 1 → 2, n → n/2 + 2 |
| `comboShadowResolution` / `ShadowQuality` / `ReflectionQuality` | — | Items from the resource; selected index = ShadowScalar / ShaderDetail / ReflectionDetail |
| `checkFullscreen`, `checkVSync`, `checkSoftwareMouse`, `checkMouseClip` | — | Video and input flags |
| `comboAutosave` | — | 9 items from `StringTable.rsc:autoSave`, index clamped 0..8 |
| `sliderUIScale` | — | 75..150; applying sets `UserInterface +0xdc` = UIScale / 100 |
| `checkStatusIcon`, `sliderStatusIcon` (0..100), `sliderStatusIconScale` (25..200) | — | Status icon options |
| `checkShowCelsius`, `checkEdgeScroll`, `checkShowWeather`, `checkAutoslow`, `checkAutopause` | — | Game flags |
| `tabOptions` | — | Active tab = `LastOption` |
| `inputList` | 13 + 2·row + slot | Rows of (`textInputNameGroup`, `buttonInputPrimary`, `buttonInputSecondary`); row r is action r + 5, named by `gameKeys` entry r + 5 |

Every video combo change runs `Video::Device::ValidateConfiguration` (`0x1800f9bc0`). An
interface or adapter out of range becomes 0. An unknown resolution becomes the desktop's
(`GetDefaultResolution`), or else the first mode. An unknown refresh rate becomes the first with
the same whole hertz, or else the first. An unsupported sample count becomes none, and a filter
above the maximum anisotropy becomes that maximum.

### Rebinding (`FUN_140077e70`)

1. A key button shows `Options.rsc:inputBind` ("Press a key to bind it to @0"). Its update
   (`FUN_140077150`) waits for a released keyboard key (codes 0..0x68) or mouse button (4..11).
   `buttonYes` is "Unbind" (result 2: the slot becomes Unbound).
2. If the input already triggers another action with a different name, `Options.rsc:inputBound`
   asks "@0 is already bound to @1. Bind it to @2 instead?". Yes clears the other binding and
   binds this one; No leaves both. With the same name, it rebinds without asking.

The port arms the box on the frame after it opens, so the release of the click that opened it does
not bind.

## Effects in the port

| Setting | Effect |
|---|---|
| UIScale | The UI is laid out in its own units and drawn at OS scale × UIScale / 100 (deviation: the original has no OS factor) |
| Volumes | `Audio::Scene` volumes, every frame |
| Status icons | Opacity, enable and scale reach the icon renderer |
| ShowCelcius | The temperature readout converts to Celsius |
| AutoPause | Losing focus opens the pause menu only when set |
| EdgeScroll | Edge scrolling within 4/5 UI pixels of the edges (see [KEYMAP-AND-CAMERA-CONTROLS.md](KEYMAP-AND-CAMERA-CONTROLS.md)) |
| ShowWeather | Off: the effects go (`FUN_1400999b0`), so the precipitation, its ambience and the weather lighting blend (`FUN_140098f90`) go with them |
| AutoSlow | A fire, blight, tornado or shortage event (types 11, 12, 30, 31) sets the speed back to 1× (`Game +0x120 → +0x34`) |
| AutosaveInterval | Stored only: saving is not ported |
| Interface, Adapter | The wgpu backends that have adapters for the window (Vulkan, Metal, DirectX 12, OpenGL, WebGPU) and their adapters; a change rebuilds the renderer |
| Width, Height, RefreshN/D, Fullscreen | Native: exclusive fullscreen when a monitor mode matches, otherwise borderless; windowed size from WindowWidth/WindowHeight. The browser lists only the canvas size and ignores Fullscreen |
| VSync | Surface present mode `AutoVsync` / `AutoNoVsync` |
| Multisample | Only "None" is offered: the port renders single-sampled |
| TextureFilter | Material samplers: bilinear, trilinear or n× anisotropic; rebuilds the renderer |
| ShadowScalar | Shadow map 2048 >> scalar; rebuilds the renderer |
| ShaderDetail | 0 no shadow pass, 1/2/3 one, four or nine shadow taps |
| ReflectionDetail | The water reflection draws the sky, plus terrain (1), plus everything (2) (`ReflectionImageBuffer::Render` passes it to its draw group) |
| MouseClipping | Cursor grab `Confined` (or `Locked` where only that exists) |
| MouseDisplay | The OS pointer is hidden and the game draws `Cursor/Arrow.rsc:cursor` at its own size with its hotspot (only the arrow; the original switches cursors per element) |

Captures ignore the stored settings. `--ui-scale PERCENT` and `--setting KEY=VALUE` (stored names,
e.g. `--setting InputMouseDisplay=true`) set them for one run.
