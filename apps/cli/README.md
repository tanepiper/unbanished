# banished-cli

Command-line tools for the game data of Banished. It has three commands: `export-audio`,
`nexus` for mods from Nexus Mods, and `banishedinfo` for mods from banishedinfo.com.

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

## nexus

Search the Banished mods on [Nexus Mods](https://www.nexusmods.com/games/banished/mods) and
install them into a `WinData` folder. This is not part of the original game, and it needs your
own Nexus Mods API key for anything but searching (`BANISHED_NEXUS_API_KEY`, or `NexusApiKey=`
in the game's settings file).

```bash
cargo run --release -p banished-cli -- nexus search terrain
cargo run --release -p banished-cli -- nexus install 19 --to path/to/WinData
```

| Command | Effect |
|---|---|
| `nexus search TEXT [--sort downloads\|endorsements\|updated\|name] [--page N]` | List matching mods |
| `nexus files MOD_ID` | List a mod's files; `*` marks the main file |
| `nexus whoami` | Show the account the key belongs to, and whether it has Premium |
| `nexus install MOD_ID [FILE_ID] [--to WINDATA]` | Download and install the mod's main file, or the file given (direct downloads need Premium) |
| `nexus install 'nxm://…' [--to WINDATA]` | Install from a Mod Manager Download link on the website |

`--to` defaults to `game-data/original/WinData`. The user guide's
[Mods from Nexus Mods](../../docs/USER-GUIDE.md#mods-from-nexus-mods) explains the key,
Premium and `nxm://` links. The library is [banished-nexus](../../crates/nexus/README.md).

## banishedinfo

List, search and install the Banished mods on
[banishedinfo.com](https://banishedinfo.com/mods/browse). This is not part of the original game.
No account is needed, and requests go to the site only when you run a command.

```bash
cargo run --release -p banished-cli -- banishedinfo list --sort downloads
cargo run --release -p banished-cli -- banishedinfo install 34 --to path/to/WinData
```

| Command | Effect |
|---|---|
| `banishedinfo list [--sort featured\|downloads\|trending\|recent] [--section NAME] [--page N]` | List the site's mods, 20 to a page |
| `banishedinfo search TEXT` | Search the mods |
| `banishedinfo versions MOD_ID` | List a mod's versions, newest first |
| `banishedinfo install MOD_ID [VERSION] [--to WINDATA]` | Download and install a version (the newest by default) |

`MOD_ID` is the number in the mod's address. The sections are `audio`, `buildings`, `gameplay`,
`maps`, `textures`, `translations` and `translated-mods`. `--to` defaults to
`game-data/original/WinData`. The [Mods](../../docs/USER-GUIDE.md#mods) section of the user
guide explains the rest, under "Mods from banishedinfo.com". The library is [banished-banishedinfo](../../crates/banishedinfo/README.md).
