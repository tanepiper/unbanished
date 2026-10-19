# banished-cli

Command-line tools for the game data of Banished. Today it has one command, `export-audio`.

## export-audio

Decodes every sound of the game (every `SoundBuffer` resource, including the streamed music) to
an MP3 file, and writes an index of them.

```bash
cargo run -p banished-cli --release -- export-audio [WINDATA] [OUT]
```

| Argument | Default | Meaning |
|---|---|---|
| `WINDATA` | `game-data/original/WinData` | The folder with the game's `.pkg` files |
| `OUT` | `game-data/derived/audio` | Where the MP3 files and `index.json` go |

It needs [FFmpeg](https://ffmpeg.org) on the `PATH`, with the LAME MP3 encoder. The game's
sounds are xWMA, which FFmpeg can decode. The command writes each sound as an xWMA file, and
FFmpeg converts it to MP3 at 128 kbit/s.

The output keeps the folders of the game: for example, `Audio\Animal\Chicken_version0` becomes
`OUT/Audio/Animal/Chicken_version0.mp3`. `index.json` lists every sound:

| Field | Meaning |
|---|---|
| `resource` | The entry in the package |
| `file` | The MP3 file, relative to `OUT` |
| `source` | The original file name, for example `Audio\Animal\Chicken0.wav` |
| `seconds` | The length |
| `looping`, `streamed` | Whether the sound loops, and whether it is streamed (music) |
| `channels`, `sample_rate` | The format |

The command stops with exit code 1 if any sound failed, and with exit code 2 if the arguments
are wrong.

`tools/extract-assets.sh` runs this command when FFmpeg is installed. The game and the viewer do
not need the MP3 files: they decode the sounds themselves with `banished-wma`. The files are for
listening outside the game, and for comparison tests.

Git ignores `game-data/`. Never commit the exported sounds.
