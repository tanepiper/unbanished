# banished-audio

Sound for the game and the viewer. It decodes the game's sounds (xWMA, with `banished-wma`)
in the program itself, and plays them on a device with 64 channels, as the original
`Audio::Device` does. On the desktop it plays through [rodio](https://github.com/RustAudio/rodio);
in the browser it plays through Web Audio. No converted sound files are needed.

| Part | What it does | Original |
|---|---|---|
| `decode`, `load_sound` | Decode a `SoundBuffer` (and the `.str` stream of music) to float samples | — |
| `Sound` | A decoded sound and its loop flag. The samples are shared between channels, not copied. | `Audio::SoundBuffer` |
| `Device` | 64 channels. `play` takes the first free channel and returns none when all are busy. | `Audio::Device`, Runtime `0x1800e0ad0` |
| `Scene` | The layer above the device that plays `SoundEffect`s: timelines, 3D positions, emitters, kinds of sound | `Audio::Scene` |
| `EffectCache` | Loads and decodes the sound effects that a scene uses, on all CPU cores natively | — |

## Use it

```toml
[dependencies]
banished-audio = { path = "crates/audio" }
banished-formats = { path = "crates/formats" }
```

On Linux, the build needs the ALSA headers (`libasound2-dev` and `pkg-config`).

### Play a sound

```rust
use banished_audio::{Device, PlayParams, Sound};
use banished_formats::pkg::Archive;

fn moo(archive: &Archive, device: &mut Device) -> Result<(), String> {
    let sound = Sound::load(archive, "Audio/Animal/Cow.rsc:version0")?;
    let params = PlayParams { volume: 0.8, pitch: 2.0, ..PlayParams::default() };
    match device.play(&sound, params)? {
        Some(channel) => println!("playing on channel {}", channel.index()),
        None => println!("all 64 channels are busy"),
    }
    Ok(())
}
```

`PlayParams` has a start offset, a volume, a pitch in semitones, and an optional loop flag (the
default is the sound's own flag). `Device` also has `stop`, `pause`, `resume`, `seek`,
`set_volume`, `set_pitch`, `set_looping`, `set_master_volume` and `update`, which frees the
channels of sounds that ended.

`Device::play` returns an error when there is no audio output. In the browser, sound starts only
after the player clicks or types on the page; `unlock` resumes the Web Audio context then.

### Decode without playing

```rust
use banished_formats::pkg::Archive;

fn length(archive: &Archive) -> Result<f32, String> {
    let (pcm, looping) = banished_audio::load_sound(archive, "Audio/Music/Menu.rsc:music")?;
    let frames = pcm.samples.len() / pcm.channels as usize;
    println!("looping: {looping}");
    Ok(frames as f32 / pcm.sample_rate as f32)
}
```

`Pcm` has the number of channels, the sample rate, and interleaved samples from -1 to 1.

### Sound effects in a scene

The game does not play sounds directly. It plays `SoundEffect` resources through a `Scene`:
an effect is a timeline of sounds with random choices, volumes and pitches. `play_effect` plays
one for the interface, and `play_effect_3d` plays one at a position in the world. Emitters
(`add_emitter`, `move_emitter`) are ambient sounds tied to a place, such as a river or a forest.
`Scene::update` mixes them every frame, through any `Mixer` (the `Device` is one).
See [AUDIO.md](../../docs/AUDIO.md) for the details.

## Tests

```bash
cargo test --release -p banished-audio
```

`tests/menu_music.rs` decodes the menu music from your game data, and skips without it.

## The API

The full API documentation is at
<https://tanepiper.github.io/unbanished/api/banished_audio/index.html>.

## Related documents

- [Audio](../../docs/topics/AUDIO.md)
- [AUDIO.md](../../docs/AUDIO.md)
- [banished-wma](../wma/README.md)
