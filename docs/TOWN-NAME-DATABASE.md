# Town-name database

This note records the decoded `NameDatabase` resources and the current new-game UI behavior.
The binary behavior below comes from Ghidra 12.1.4 analysis of the Steam x64 executable and the
resource layout was checked against the extracted game data. Static decompilation is not runtime
observation.

## Resource layout

The engine registers `NameDatabase` (`.ndb`) as an `Object::External` subtype. Its serializer is
`FUN_1400a9ab0` and writes four fields in this order inside the class block:

1. A signed count followed by 9-byte transition records: a 4-byte ASCII prefix (short personal-name
   prefixes are NUL padded), a one-byte boundary marker, and a 32-bit next-character bit mask.
2. A signed count followed by 16-bit indices into the transition table. The game samples this array
   to select a starting prefix.
3. A signed byte count followed by the packed NUL-terminated string pool.
4. A signed count followed by 32-bit offsets into that pool, sorted by name.

The transition mask uses bits 0 through 25 for `a` through `z`, and bit 26 for termination. The
game's `FUN_1400a9d50` samples a start entry, walks transition records, and composes a candidate.
`FUN_1400aa000` retries candidates found in the accepted-name pool, for at most 50 attempts.
`FUN_14009ef90` selects `Game/Names/Cities.ndb`, requests a minimum of five characters, and applies
a special suffix adjustment to names ending in `vill`.

Observed resource counts:

| Resource | Transitions | Start indices | String bytes | Accepted names |
|---|---:|---:|---:|---:|
| `Cities_resource.crs` | 9,048 | 2,587 | 38,811 | 4,518 |
| `Male_resource.crs` | 4,381 | 1,388 | 15,694 | 2,177 |
| `Female_resource.crs` | 4,136 | 1,412 | 20,490 | 2,795 |

The Rust decoder is [`crates/formats/src/names.rs`](../crates/formats/src/names.rs). It validates
the resource type, pool offsets, NUL termination, transition masks, start indices and sorted name
order. The game currently reads the city resource through the package archive when the menu loads.

## Generator (ported, bit-exact)

`crates/game/src/names.rs` ports the generator from the disassembly.

- **`FUN_1400a9d50`** (one candidate):
  1. Draw a start record with `Rand(start count)`. Copy its 4-byte prefix and uppercase the first
     letter (−0x20).
  2. Draw a target length with `Rand(min, max − 4)`.
  3. Run up to `2·max − 3` steps. Each step:
     - binary-search the transition table for the current 4-character window (big-endian,
       unsigned compare);
     - stop if the window is missing or its mask is empty;
     - at or past `max`, stop at a boundary record; otherwise a boundary records the cut point
       `written + 4`;
     - pick the next letter as the `Rand(popcount(mask & 0x7ffffff))`-th set bit;
     - end the name (NUL at `written + 4`) when that bit is 26, or when past the target length
       and bit 26 is allowed;
     - otherwise append the letter and slide the window.
  4. If all steps run out, truncate at the last boundary.
- **`FUN_1400aa000`** (retries): up to 50 candidates. It keeps the first one that has at least
  `min` letters and is not in the sorted shipped-name pool (`String::StrCompare`, a signed-char
  strcmp). After 50 tries it returns the last candidate.
- **`FUN_1400aa130`** (citizens): `Male.ndb` or `Female.ndb`, 3 to 10 letters.
- **`FUN_14009ef90`** (towns):
  - `Cities.ndb`, at least 5 letters, with maximum `(editName max length / 3) · 2`
    (`FUN_140075ba0`);
  - a name ending in `vill` gets `e` appended (string literals at `0x140165078` and `0x140165080`).

### Verification

`tools/emu/names.py` runs the original `FUN_1400aa000` under the emulator. It generates 25
consecutive names for each of seeds 1–40 in each database. `crates/game/tests/names_emu.rs`
matches all 3,000 exactly, which also confirms that every random draw happens in the original
order.

The New Game dialog now uses the town generator. The dialog's stream (`+0x118`) is the engine's
default clock-seeded `Random()`.
