# unbanished-wma

A decoder for Windows Media Audio version 2 (WMA v2, format tag `0x161`), written in Rust.
WMA v2 is the codec inside xWMA files. Every sound in Banished is xWMA.

- No dependencies and no `unsafe` code.
- Builds for `wasm32-unknown-unknown`, so the browser version decodes the game's sounds itself.
  It does not need converted files.
- Checked against FFmpeg's `wmav2` decoder on every sound of the game: the signal-to-noise ratio
  must be at least 60 dB (`tests/ffmpeg_parity.rs`).

## Use it

```toml
[dependencies]
unbanished-wma = { path = "crates/wma" }
```

### Decode a whole stream

xWMA carries no codec extra data, so the parameters come from the `WAVEFORMATEX` header of the
sound: the number of channels, the sample rate, the average bytes per second, and the packet
size (`block_align`). `Params::xwma` sets the codec flags that xWMA implies.

```rust
use unbanished_wma::{decode, Params};

fn to_pcm(packets: &[u8]) -> Result<Vec<i16>, unbanished_wma::Error> {
    // 2 channels, 44,100 Hz, 6,000 bytes per second, 2,230-byte packets.
    let params = Params::xwma(2, 44_100, 6_000, 2_230);
    decode(&params, packets) // interleaved 16-bit samples
}
```

`decode` works like FFmpeg: a packet that fails to decode is skipped, and the output ends with
one extra frame that holds the final overlap.

### Decode packet by packet

For streaming, use a `Decoder`. It writes float samples in the range -1 to 1.

```rust
use unbanished_wma::{to_i16, Decoder, Params};

fn stream(params: &Params, packets: &[u8]) -> Result<Vec<i16>, unbanished_wma::Error> {
    let mut decoder = Decoder::new(params)?;
    let mut pcm: Vec<f32> = Vec::new();
    for packet in packets.chunks(usize::from(params.block_align)) {
        decoder.decode_packet(packet, &mut pcm)?;
    }
    decoder.flush(&mut pcm); // the last overlap
    Ok(to_i16(&pcm))
}
```

### Decode a sound from the game

`unbanished-formats` reads the sound resource. `unbanished-audio` does both steps in one call
(`unbanished_audio::load_sound`).

```rust,ignore
use unbanished_formats::pkg::Archive;
use unbanished_formats::types::sound_buffer;
use unbanished_wma::{decode, Params};

fn chicken(archive: &Archive) -> Result<Vec<i16>, Box<dyn std::error::Error>> {
    let sound = sound_buffer(&archive.read_ref("Audio/Animal/Chicken.rsc:version0")?)?;
    let f = &sound.format;
    let params = Params::xwma(f.channels, f.sample_rate, f.avg_bytes_per_sec, f.block_align);
    Ok(decode(&params, &sound.data)?)
}
```

## The API

| Item | What it is |
|---|---|
| `Params` | The stream's parameters. `Params::xwma(channels, rate, bytes_per_sec, block_align)` |
| `decode(&Params, &[u8])` | Decode a whole stream to interleaved `i16` |
| `Decoder` | `new`, `decode_packet`, `flush`, `reset`, and `channels`, `sample_rate`, `delay`, `frame_len` |
| `to_i16(&[f32])` | Convert float samples to 16-bit, with rounding and clipping |
| `Error` | `Unsupported` (a valid stream that uses a feature this decoder lacks) or `Invalid` |
| `WMAV2`, `XWMA_FLAGS` | The format tag and the flags that xWMA implies |

The full API documentation is at
<https://tanepiper.github.io/unbanished/api/unbanished_wma/index.html>.

## Source files

| File | What it does |
|---|---|
| `src/lib.rs` | `Params`, `decode`, `to_i16`, `Error` |
| `src/decoder.rs` | Super-frames, frames and blocks |
| `src/mdct.rs` | The inverse MDCT, through a DCT-IV with a half-size complex FFT |
| `src/vlc.rs` | The table-driven decoder for prefix codes |
| `src/bits.rs` | The bit reader. Reads past the end give zero bits, as WMA frames allow. |
| `src/tables.rs` | The constant tables that define the bitstream (codewords, band layouts) |

## Tests

```bash
cargo test --release -p unbanished-wma
```

`ffmpeg_parity` decodes every sound of your game and compares the result with FFmpeg. It skips
when `game-data/original/WinData` or `ffmpeg` is missing.
