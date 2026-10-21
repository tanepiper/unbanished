# Audio

Banished has music, ambient sounds (forest, river, wind, rain), sound effects in the world
(animals, work, fire, tornadoes) and interface sounds. Un-Banished plays all of them from the
original files, on the desktop and in the browser. This page explains the parts. The code is in
[unbanished-audio](../../crates/audio/README.md) and [unbanished-wma](../../crates/wma/README.md).

<p align="center">
  <img src="../assets/screenshots/viewer-audio.webp" alt="The viewer's Audio mode, playing a chicken sound" width="720">
</p>

## Sound files

A sound is a `SoundBuffer` resource. It holds a format header, a loop flag, the length, and the
data in **xWMA**: Windows Media Audio version 2, in a container that Microsoft's XAudio2 uses. The
16 music tracks are *streamed*: their data is in a separate raw entry (`.str`) in the package.

Browsers cannot play xWMA, and no Rust library decoded WMA v2. So Un-Banished has its own
decoder, `unbanished-wma`: pure Rust, no dependencies, no `unsafe`, and it builds for WebAssembly.
A test decodes every sound of the game and compares it with FFmpeg's decoder; the
signal-to-noise ratio must be at least 60 dB.

## The device

The original plays sound on an `Audio::Device` with 64 channels. `Play` takes the first free
channel, and returns nothing when all 64 are busy, so the 65th sound at the same time is not
heard. The port keeps the same rule. On the desktop, each channel is a rodio sink; in the
browser, each channel is a Web Audio source node with its own volume. The samples of a sound are
decoded once and shared by all channels that play it.

Pitch is given in semitones: a pitch of `p` plays at `1.05946314^p` times the normal speed, as in
the original.

## Sound effects and the scene

The game rarely plays a sound directly. It plays a **sound effect** (`SoundEffect` resource): a
timeline of sounds, each with several variants, a pitch range and a volume. When an effect starts,
the scene chooses a variant and a pitch with random numbers, and starts each sound at its time.

The `Audio::Scene` has 256 slots for playing effects, and 4,096 slots for effects in the world.
An effect in the world has a position. Its volume is 1 inside its inner radius and falls in a
straight line to 0 at its outer radius, measured from the camera. *Emitters* are ambient sounds
tied to places: a river, a forest, a field, a burning house. An effect with several emitters
plays at the volume of the loudest one.

Every sound has a type. Each type has its volume in **Options → Audio**:

| Type | Sound | Default volume |
|---|---|---|
| 1 | Interface | 100 |
| 2 | Music | 50 |
| 3 | Ambient | 100 |
| 4 | Effects | 100 |

## In the browser

Browsers start sound only after the player clicks or types on the page. So the browser version
starts its Web Audio context on the first click. Until then, no sound plays.

## Read more

- [AUDIO.md](../AUDIO.md): the device, the scene and the effects in detail.
- [STARTING-AUDIO-EMITTERS.md](../STARTING-AUDIO-EMITTERS.md): the emitters of a new town.
- [The viewer](../../apps/viewer/README.md): listen to every sound of the game.
- [unbanished-cli](../../apps/cli/README.md): export every sound to MP3.
