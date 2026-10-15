# Audio device

The game and the viewer share one audio engine: `crates/audio`. It holds the decoding
(`decode`, `load_sound`, xWMA through `banished-wma`), the `Sound` buffer, and the 64-channel
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

## Not ported

- `Audio::Scene`, the layer above the Device. It covers `PlayEffect` and `PlayEffect3D` with
  a `SoundType`, emitters, attenuation, per-type pause, and `StopAll`. The menu's `StopAll` is
  `Device::stop_all` for now.
- `SoundEffect` resources: variants, random pitch and volume.
- Packet-granular start offsets (the port starts at the exact sample) and the `Stream` path.
  The port decodes streamed music whole.

## Verification

- `crates/audio` tests:
  - first-free allocation, reuse after a channel is freed, stale handles, and the 64-channel
    limit;
  - offset wrapping and the semitone ratio;
  - the shared source looping, seeking and sharing one buffer.
- The game's menu music and the viewer's preview both play through the same `Device`.
- Both apps build for native and WASM.
