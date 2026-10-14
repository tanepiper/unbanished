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
`Save2`, `(0x159a4, 3)` for `Auto`, and `(0x15bc6, 3)` for `allportez`. The metadata is inside the
first compressed payload; after expanding that stream, the marker begins the metadata block. The
stream has a 28-byte table (a count of three plus three 8-byte entries), followed by LZF bytes. The
ordinary `.crs` whole-file parser therefore cannot be applied to the save file as if it were a
standalone resource.

## Runtime write path

`Save::SaveLoad::UpdateSave` (`Runtime` address `0x1800c52f0`) serializes metadata first through
`SaveMetaData` (`0x1800c3d00`), then creates/uses the game `SavedState`, and submits two block
descriptors to `IO::Persistence::BeginSave` (`0x1800ba270`). This establishes two logical save
blocks. The observed file header contains a metadata block ID value `0x24` and a game-state ID value
`1`; matching those on-disk fields to the descriptors is strongly indicated by the decompilation,
but the enclosing header layout still needs to be decoded before treating the mapping as final.

Both payloads use the engine's `IO::CompressedStream` table: `u32 block_count`, followed by entries
`{u32 data_offset, u16 compressed_size, u16 raw_size}`, then LZF or stored block bytes. Offsets are
relative to the beginning of that payload. For the first payload, all four tables have three
windows with raw sizes `0x8000`, `0x8000`, and a final partial window. The compressed bytes expand
to one metadata block with the ordinary `0xAAAAAAAA`, byte-size, version header and a final
`0xBBBBBBBB` terminator. The metadata block size is the expanded payload length plus that
terminator; the earlier version 2 sample ends immediately after its terminator and has no mod list.

The second payload is also a valid `CompressedStream`, and every LZF block expands to its declared
size without exceeding the engine's `0x8000` window. Its whole stream then expands to a version 3
block of `12 + block_size` bytes, ending in `0xBBBBBBBB`. The four gameplay snapshots expand to
about 25.6–33.2 MiB. The block counts are 881 (`starter-save`), 1,064 (`Save2`), 1,063 (`Auto`),
and 819 (`allportez`); each table accounts for the exact declared payload size. This verifies the
two compression layers and file boundaries, but the object records inside the game-state block are
still not decoded.

| File | First stream blocks / raw bytes | Metadata name | Preview | Metadata extension value | Mod count |
|---|---:|---|---:|---:|---:|
| `starter-save.sav` | 3 / 88,108 | `Aurelin` | 172×128 RGBA | absent (metadata version 2) | absent |
| `Save2.sav` | 3 / 88,484 | `Driscola` | 172×128 RGBA | 2 | 4 |
| `Auto.sav` | 3 / 88,496 | `Автосохранение` | 172×128 RGBA | 2 | 4 |
| `allportez.sav` | 3 / 89,042 | `Allportez` | 172×128 RGBA | 2 | 17 |

Version 3 metadata stores the extra 32-bit value after the image, then a signed mod count and
records. Each record has two strings and three 32-bit values. The strings identify a displayed mod
name and a `.pkm` file; the meanings of the three integers remain unknown. `Save2` and `Auto` list
the same four mods: Cheat Mod, Fast Road Mod Test, Translation Russian, and Unlimited mines and
quarries. `allportez` lists 17 mods, including Animal Example, Better Schools, Cheat Mod, Debug
Menu, Unlimited mines and quarries, Building Example, Camellia Crop, Colorful 2 Floors Little
Houses, Crop Example, Dairy Milk and Creamery, Fig Example, Japanese Houses, Jatropha integerrima
Crop, New Medieval Town version 1.1b, Rose Crop, Stone Bridge, and Translation To Russian.

`SaveMetaData` writes, in order, the save name, image width, image height, `width * height * 4`
image bytes, a 32-bit field, then a count and per-mod strings/integers for the current metadata
version. It constructs a
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
compressed stream and deserializes the loop. The decoded stream begins with a version 3 block whose
first nested block is version 0. In `Save2`, that nested block starts at expanded offset `0x0c`, has
size `0x7c22c`, and ends at `0x7c244`; another small version 0 block follows at that boundary. The
first nested block starts with `0x1567`, `1`, and a length-prefixed UTF-16 path
`Audio\Animal\Chicken.rsc:event`. This resembles a counted serialized reference collection, but
the count and record type have not been confirmed from the owning serializer. These objects still
need to be mapped to game types before a gameplay-state decoder can be built.

## Open work

- Determine how the version 1 header's implicit first block ID is selected.
- Identify the timestamp and flags and establish version compatibility rules.
- Trace game deserialization order and identify the nested object types and which world fields are
  serialized versus rebuilt.
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
