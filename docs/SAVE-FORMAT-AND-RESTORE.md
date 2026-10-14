# Save container and restore path

This note records the first static trace of Banished `.sav` files and the Runtime save code. It is
based on four user-supplied save files and Ghidra 12.1.4 decompilation of the Steam x64
`Runtime-steam-x64.dll`; it is not a runtime restore test or a complete decoder. Ghidra exports and
the projects remain under ignored `analysis/`.

## Save files examined

| File | Bytes | SHA-256 | Initial format pair | Metadata block version |
|---|---:|---|---|---:|
| `starter-save.sav` | 12,921,041 | `10a59afbcdfb4238040400f664c20a3d42654529984cb331b164b379339056f9` | `1, 2` | 2 |
| `Save2.sav` | 15,303,280 | `896d28ddbfcff14f43a2eb5b6688d6d1c07c1ad415a1a57dce2a9bcd450bf5fa` | `2, 2` | 3 |
| `Auto.sav` | 15,306,375 | `e9884af36891cea7646c2f8c61a855ef5c732e823470051f17ffdf7d1cb35b34` | `2, 2` | 3 |
| `allportez.sav` | 12,172,879 | `15256bf48a32f713aa61c270c8412d426e94ce110e7bd1f768261fadfb571653` | `2, 2` | 3 |

The first two little-endian 16-bit values are stable within these files except for the first value
(`1` in `starter-save`, `2` in the other three); the second is `2` in all four. The header gives
two contiguous payloads, and their offsets and lengths reconcile exactly with the file sizes:

| File | First payload offset/size | Second payload ID/offset/size | End check |
|---|---|---|---|
| `starter-save.sav` | `0x1c` / `0x13f32` | `1` / `0x13f4e` / `0xc3e983` | second payload ends at EOF |
| `Save2.sav` | `0x24` / `0xe569` | `1` / `0xe58d` / `0xe89ce3` | second payload ends at EOF |
| `Auto.sav` | `0x24` / `0x120cd` | `1` / `0x120f1` / `0xe86d96` | second payload ends at EOF |
| `allportez.sav` | `0x24` / `0x1473a` | `1` / `0x1475e` / `0xb876f1` | second payload ends at EOF |

For version 2 files, the fixed first payload offset is `0x24`; the header at `0x10` stores the
first block ID `0x24` and its length, then the second block ID (`1`), offset and length. The first
payload length places the second payload exactly at the stored offset. The observed version 1 file
stores its first payload offset and length at `0x08` and `0x0c`, then the second block ID, offset and
length at `0x10`, `0x14` and `0x18`. Its first block ID is not explicit in this layout. Bytes 4–7
are zero in the version 1 file and `80 80 80 80` in the version 2 files; version 2 also has a
32-bit value at `0x08` and a zero word at `0x0c`, whose meanings remain unknown. The timestamp,
flags, and full compatibility rules have not been established.

Each file contains a metadata block beginning with `0xAAAAAAAA`. It starts at file offset `0x39`
in `starter-save.sav` and `0x41` in the other three. The following little-endian 32-bit words are
the block byte size and metadata version: `(0x15820, 2)` for the starter save, `(0x15998, 3)` for
`Save2`, `(0x159a4, 3)` for `Auto`, and `(0x15bc6, 3)` for `allportez`. The bytes before the
marker differ by save version and include a 28-byte region in the observed samples, but its table
fields have not yet been decoded. The ordinary `.crs` whole-file parser therefore cannot be applied
to the save file as if it were a standalone resource.

## Runtime write path

`Save::SaveLoad::UpdateSave` (`Runtime` address `0x1800c52f0`) serializes metadata first through
`SaveMetaData` (`0x1800c3d00`), then creates/uses the game `SavedState`, and submits two block
descriptors to `IO::Persistence::BeginSave` (`0x1800ba270`). This establishes two logical save
blocks. The observed file header contains a metadata block ID value `0x24` and a game-state ID value
`1`; matching those on-disk fields to the descriptors is strongly indicated by the decompilation,
but the enclosing header layout still needs to be decoded before treating the mapping as final.

`SaveMetaData` writes, in order, the save name, image width, image height, `width * height * 4`
image bytes, a 32-bit field, then a count and per-mod strings/integers. It constructs a
`CompressedMemoryStream`, requests two compression windows, and stores the resulting memory stream
through `SavedState::Store`. `LoadMetaData` (`0x1800c43a0`) reopens that compressed stream and reads
the corresponding name, dimensions, image, and metadata version; it reads the mod list only when
the version exceeds 2. The on-disk marker's version 2/3 values agree with this compatibility branch.

`SavedState` is a simple byte-buffer holder: its fields are ID, byte count, and buffer pointer.
`Store` copies the entire `MemoryStream` contents; `Retrieve` opens a memory stream of the stored
size and copies those bytes back. `SetupLoad` obtains the saved block's size from Persistence's
loaded block table, allocates the buffer and returns a `{pointer, size, id}` descriptor. The
Persistence worker performs file IO asynchronously after `BeginSave`/`BeginLoad` signal it.

## Restore handoff

The MainMenu's `buttonLoad` opens `Dialog/SaveLoad.rsc` in mode 1. `Save::SaveLoad::UpdateLoad`
(`0x1800c4f00`) requests save metadata, checks installed modifications against the saved mod list,
then prepares a `SavedState` and requests the game-state block. `UpdateLoadFromFile`
(`0x1800c4dd0`) advances the async file-load operation and moves to completion when the request is
ready. The game-side initialization path accepts a non-null saved-state object and transfers it to
the game loop; see the save-state call sites in `analysis/gameplay-gameloop-methods.txt` and
`analysis/gameplay-world-init.txt`.

The game loop serializes its own payload through `FUN_14009ae90`: the save side writes the loop to a
`CompressedMemoryStream` and stores it in `SavedState`; the load side retrieves the bytes, opens the
compressed stream and deserializes the loop. This means `.sav` recovery needs both the outer
Persistence file framing and the inner memory-stream format before the gameplay payload can be
decoded faithfully.

## Open work

- Identify the purpose of the first payload's internal 28-byte region and determine how the
  version 1 header's implicit first block ID is selected.
- Identify the timestamp and flags and establish version compatibility rules.
- Extract both logical payloads and validate their declared sizes and compression windows against
  `IO::CompressedStream::Open`.
- Decode metadata fields and identify the image, save name, game dimensions, timestamp and mod
  records in each sample.
- Trace game deserialization order and identify which world fields are serialized versus rebuilt.
- Check older/newer version behavior against a real game load before implementing a writer.

Useful Runtime Ghidra locations: `Save::SaveLoad::UpdateSave` (`0x1800c52f0`),
`Save::SaveLoad::UpdateLoad` (`0x1800c4f00`), `Save::SaveLoad::UpdateLoadFromFile`
(`0x1800c4dd0`), `Save::SaveLoad::SaveMetaData` (`0x1800c3d00`),
`Save::SaveLoad::LoadMetaData` (`0x1800c43a0`), `IO::Persistence::BeginSave` (`0x1800ba270`),
`IO::Persistence::BeginLoad` (`0x1800ba1d0`), and `IO::CompressedStream::Open` (`0x1800ab4b0`).

To repeat the named-function export against the analyzed Runtime project:

```sh
"$GHIDRA_INSTALL_DIR/support/analyzeHeadless" analysis/save-project banished-save \
  -process Runtime-steam-x64.dll -noanalysis -scriptPath tools/ghidra \
  -postScript ExportNamedFunctions.java /tmp/save-runtime-functions.txt \
  SaveLoad SavedState Persistence CompressedStream
```
