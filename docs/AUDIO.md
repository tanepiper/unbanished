# Audio device

The game and the viewer share one audio engine: `crates/audio`. It holds the decoding
(`decode`, `load_sound`, xWMA through `unbanished-wma`), the `Sound` buffer, and the 64-channel
`Device`. The viewer's single-sound preview (`apps/viewer/src/player.rs`, used for play, pause,
seek, loop and volume) is one channel on that `Device`. The game uses the `Device` directly
through `Ctx.audio`.

Evidence: Ghidra pseudocode of `Runtime-steam-x64.dll`.

## `Audio::Device`

| Function | Behaviour | Port |
|---|---|---|
| `Device::Device` `0x1800e0bf0` | 64 channels of `0x28` bytes, plus one `Stream` for streamed buffers | `CHANNELS`, `Slots` |
| `FindFreeChannel` `0x1800e09d0` | lowest channel whose flag bit 0 is clear, else -1 | `Slots::insert` |
| `Play` `0x1800e0ad0` | first free channel; `PlayPlatform`; marks it in use, stores the buffer and the end callback with its two arguments; null when none is free | `Device::play` → `Ok(None)` when full |
| `PlayPlatform` `0x1800e0ff0` | XAudio2 source voice; start at `time % duration` (in bytes, rounded to an xWMA packet); loop count 255 when the buffer's loop flag (`+0x20`) is set; `SetVolume(volume)`; `SetFrequencyRatio(1.05946314^pitch)`; streamed buffers (`+0x21`) are fed by `Stream::Play` | `PlayParams { offset, volume, pitch, looping }`, `Sound::wrap`, `pitch_ratio` |
| `Update` `0x1800e0d70` | channels flagged ended (bit 1) are freed and their callbacks called | `Device::update` returns the freed channels; `MainLoop::update` calls it every frame |
| `Stop` `0x1800e0a50` | stop, free, callback | `Device::stop` |
| `SetChannelVolume` | per-channel volume | `Device::set_volume` |

Channel handles carry a generation. A handle kept after its channel was freed and reused
does nothing.

- **Native:** one rodio output stream. Each channel is a `Sink` on its mixer playing a
  `SharedSource` over the shared samples, so no channel copies them. Pitch is `Sink::set_speed`,
  which changes speed and pitch together, like XAudio2's frequency ratio.
- **Web:** one `AudioContext`. Each channel is an `AudioBufferSourceNode` and a `GainNode` feeding
  a master gain. The decoded `AudioBuffer` is cached in the `Sound`. A channel's position is
  computed from the context clock and its playback rate. Pausing a channel stops its source and
  keeps its offset; the other channels keep playing.

The master volume scales every channel. Captures set it to 0.

## `Audio::Scene` (`crates/audio/src/scene.rs`)

The Scene plays `SoundEffect`s on the Device. Evidence: `Scene::Update` `0x1800e1f10`, `PlayEffect`
`0x1800e17b0`, `PlayEffect3D` `0x1800e1700`, `FindFree3DEffect`, `GetAttenuatedVolume`,
`GetVolumeForPosition`, the emitter functions, `Pause`, `PauseEffect`, `StopAll`, `StopEffect`,
`StopEffect3D` and `SoundEffect::Serialize`/`GetSoundBuffer`/`Compile`.

- **SoundEffect** (`crates/formats/src/audio.rs`): radius `+0x20`, inner radius `+0x24`, a
  timeline of sounds and the max length `+0x28` in microseconds. Each sound has a start time
  and variants; each variant has a buffer, a pitch range in semitones and a volume. The max
  length is 0 when a buffer loops. All 46 shipped effects parse.
- **2D effects:** 256 slots (`0x70` bytes). Each update, every sound whose start falls in
  `[previous elapsed, elapsed + dt)` starts once. Its variant is `Rand() % n`, its pitch is `min
  + (max - min) · Rand() · 2^-32`. Its volume is `variant × effect volume (set by 3D) × type
  volume`. Running sounds follow volume changes.
  When every sound has ended (the Device's end callback), the effect frees itself and the 3D
  effect it served.
- **Sound types** (the `Configuration` volumes; type 0 never plays):

  | Type | Sound | Default volume |
  |---|---|---|
  | 1 | UI | 100 |
  | 2 | music | 50 |
  | 3 | ambient | 100 |
  | 4 | effects | 100 |

  `Pause(type)` stops that type's channels. Started sounds do not restart.
- **3D effects:** 4096 slots. The volume is the loudest emitter (or the effect's own position).
  It is linear from 1 inside the inner radius to 0 at the radius, plus the listener's reach.
  At 0 the 2D effect stops; above 0 it plays, starting at the 3D effect's elapsed time.
  - A 3D effect ends after its max length.
  - The same effect is not restarted within its radius for `max / 20` or the frame step.
- **Handles:** a 2D handle is `i·2 | counter<<9`, a 3D handle `i·2 | 1 | counter<<13`.

## Game sounds

| Sound | Original | Port |
|---|---|---|
| Menu music `Audio/Music/Menu.rsc` | `FUN_1400a0e00` `PlayEffect(…, 2)`; menu `Scene::Update(dt, 0, …, origin, 0)` | `menu.rs` |
| In-game music | `FUN_1400a95b0`: when the track has ended, a `Game/MusicTracks.rsc` track whose months hold the current month and whose mood range holds `(health/max + happiness/max)/2`; `Rand(n)`, the next one if it repeats the last | `music.rs` |
| Scene update | `FUN_14009b850`: `Scene::Update(dt, dt × speed × 6, Random, camera pivot, eye–pivot distance)`; `speed` is the iteration count `+0x34`, read even while paused | `gameplay.rs` `update_audio` |
| Pause menu | `FUN_1401071b0` opens `Dialog/Pause.rsc` as a `MainMenuDialog` bound to the game (toolbar Options, Escape, or the controller `FUN_140107b70` when the window loses focus). Its open (`FUN_1400b91a0`) pauses types 4, 3, 2; its close (`FUN_1400b9130`) resumes them. The dialog pauses the game (`PausesGame`, flags bit 0), so `FUN_14009b850` skips the simulation and the controller; music and `Scene::Update` keep running | `pause_menu.rs`, `gameplay.rs` |
| Menu focus | `FUN_1400a0bb0`: while `Application::IsPaused` (the window lost focus: `+0x11` cleared on WM_KILLFOCUS) the menu pauses type 2 and does not update its scene | `menu.rs`; `Ctx::app_paused` from winit `Focused` |
| UI clicks | `ButtonElement::Update`, `SliderElement::Update`, `ScrollElement::Update`: `Play(4)` on press, `Play(5)` on click; `ComboDialog::OnElementEvent` `Play(5)`; slots set by `FUN_1400778f0` (2 BeginDrag, 3 EndDrag, 4 ButtonDown, 5 ButtonUp, 6 Alert) | `Ui::sounds` → `Audio::play_ui` |
| UI slots 2, 3, 6 | Never played in 1.0.7. `Play(2)` is only in `UserInterface::BeginDragDrop` (`0x180121e80`), which nothing calls: the exe does not import it and the DLL has no caller. `Play(3)` is only in `Element::Update` when a drag-drop ends, which needs one begun. No `Play(6)` exists in the DLL or the exe (the exe's only `Play` is `Play(4)` in the SaveLoad list hover `FUN_1400bbc60`) | slots loaded, nothing to trigger |
| Placement | `FUN_1401050c0` move (new cell, every 0.05 s at most), `FUN_1401055c0` thunk, `FUN_140105610` error | `gameplay.rs` |
| Event alert | `FUN_140068590`: `WarningEffect` when the ribbon gains a notification | `gameplay.rs` |
| Animation sounds | `AnimAudioEvent` (`FUN_14008d310`): `RandFloat()` on the game Random against the probability, then `PlayEffect3D(effect, 4)` at the entity (tree falls, tools, animals) | the draw is in the simulation (`seeding.rs` `animate`); the sound plays on the next audio update |
| Rain | `FUN_140099790` registers `RainEffect.rsc:ambient` (autostart) at the origin when the Rain effect starts; `FUN_1400999b0` sets its first slot's volume to the effect's intensity every iteration and removes it at 0. Snow has no ambient | `environment.rs` effects, `ambience.rs` `weather` |
| Building emitters | Templates with an `ambientemitter` component (FireEffect in slot 0 of most buildings; CrowdEffect in slot 1 of WoodHouse, StoneHouse, Hostel; autostart off). Fire: `FUN_140049f70` starts the FireDescription `+0x2c` slot at the model position (with fire event 0x1f), `FUN_140049e20` removes it. Crowd: `FUN_14012fbe0` (first resident) and `FUN_14012ee40` (residence update while occupied) start slot 1 at volume 1; `FUN_14012fa20` (last resident leaves) removes it | `ambience.rs` `sync_entities` registers them; `drive_entities` starts and stops the fire slot with `burn > 0` and the crowd slot with residents, each frame |
| Tornado | `Template/Tornado.rsc:ambientemitter` (`TornadoEffect`, autostart): plays while a tornado entity exists. The tornado is spawned by the environment (`FUN_1400999b0` → `FUN_1400ae760`) | registered like the buildings once the tornado exists; follows it |
| Map ambience | `FUN_1400a57c0`: an Environment emitter per 16-cell region (`FUN_140043c70`: heaviest terrain class, warm/cold effects cross-faded over 31–36 °F), the wind at height 30 (Medium), river emitters every 8 units of river path (`Game +0x110 → +0x1e0`: the generator's paths, reversed, filled by `FUN_1400b1980`; see STARTING-AUDIO-EMITTERS.md) | `ambience.rs`; rivers from `World::river_paths` |

## Deviations

- **Separate Random for audio.** `Scene::Update` and the music player take the game's Random
  (`Game +0xd8`). Their draws happen once per frame, depending on frame timing and on when
  sounds end, so the original's simulation stream is not reproducible there anyway. The port
  gives audio its own Random so the simulation stays deterministic. The `AnimAudioEvent`
  probability draw happens inside the simulation, so it stays on the game Random.
- **Ambience as an audio layer.** The original registers the ambient emitters as entities in
  the entity manager (the environment in bucket 2). The port does not register them, and
  re-weighs a region when the map changes rather than on its scheduled visit.
- **One Scene.** The menu loop owns its own Scene (`+0xb30`) in the original; the port shares
  one, so the menu resumes music on shutdown and the pause menu resumes its types before
  returning to the main menu.

## Not ported

- The SaveLoad dialog's hover click (`FUN_1400bbc60`, `Play(4)` when the hovered save row
  changes): the dialog is not ported.
- The pause menu's Load, Save and Mod buttons (the other buttons and the Save/Mod disabling
  by game flag bit 10 are in `pause_menu.rs`).
- Packet-granular start offsets (the port starts at the exact sample) and the `Stream` path.
  The port decodes streamed music whole.

## Verification

- `crates/audio` tests:
  - first-free allocation, reuse after a channel is freed, stale handles, and the 64-channel
    limit;
  - offset wrapping and the semitone ratio;
  - the shared source looping, seeking and sharing one buffer.
- The game's menu music and the viewer's preview both play through the same `Device`.
- `crates/audio` scene tests: timeline windows and freeing, type volumes and silent type 0,
  3D attenuation over emitters, pause and resume.
- `crates/formats/tests/audio.rs`: every SoundEffect, MusicList, AnimAudioEvent and ambient
  description parses.
- `unbanished-game` test `rain_fades_in_and_out`; `apps/game` tests
  `rain_effect_drives_the_rain_emitter` (the emitter follows the effect and leaves with it) and
  `building_emitters_register_without_playing` (registered silently; a started slot plays and
  stops); `crates/formats` test `weather_descriptions_parse`.
- `apps/game` test `pause_menu_pauses_sounds_and_the_simulation`: Escape opens the pause
  menu, which pauses types 2–4 (not UI) and leaves the calendar and the game Random untouched;
  Escape resumes.
- `apps/game` test `audio_plays_music_ambience_and_keeps_the_simulation_stream`: a generated
  start plays a music track and the ambience. `tools/triage/determinism.sh` stays identical.
- Both apps build for native and WASM.
