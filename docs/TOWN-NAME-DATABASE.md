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

## Current UI behavior and fidelity gap

Opening New Game and pressing `buttonNewName` both select a name from the decoded accepted-name
pool and place it in `editName`. The current selection is uniform over the accepted names. This is
deliberately a simple first implementation: it produces valid shipped town names but does not
reproduce the game's transition-walk probabilities, min/max length choices, 50-attempt collision
behavior, or `vill` suffix adjustment. Male and female databases are decoded but are not used by
this UI path.

The exact generator can be ported later from `FUN_1400a9d50` and `FUN_1400aa000` if matching the
original distribution is needed. The serializer and resource schema are no longer blockers.
