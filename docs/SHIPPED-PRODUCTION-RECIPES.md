# Shipped production recipe data

## Source and decoding

This catalog covers every `ConsumeProduceDescription` resource found by scanning
the four shipped Windows packages. Six matching resources are in `data0.pkg`;
`data1.pkg`, `patch0.pkg`, and `patch1.pkg` contain none. The source package
SHA-256 is recorded in `game-data/MANIFEST.sha256`:
`8e3064ee0d7c89f41ac2ba26beee046801d463030fb40a0710a01548bc01debb`.
The application schema was checked against Ghidra 12.1.4 decompilation of
`Application-steam-x64.exe` (SHA-256
`323e0a12d1ef36605db48d44210745e7043240f07d2d0ae2cf7e13e3c727a5ed`).

`ConsumeProduceDescription::Serialize` at `0x14011e5b0` serializes an optional
string-table reference at `+0x20`, then a counted array at `+0x28`/`+0x30`.
`0x140033050` reads recipe records with a `0x28`-byte stride:

| Record offset | Serialized contents |
| --- | --- |
| `+0x00` / `+0x08` | Counted list of input resource-template references and signed quantities; each pair is 16 bytes |
| `+0x10` | Output resource-template reference |
| `+0x18` | 32-bit group/type value, also used as an index in the manager counter table by `0x1400ecd10` |
| `+0x20` | String-table key for the alternative's UI label; empty where no label is supplied |

The reader consumed every recipe block exactly, with no remaining bytes. English
labels below come from `Dialog/StringTable_rawMaterial.crs`. Resource quantities
and output fields are package data; their gameplay meaning is not inferred from
the label text.

## Recipes and output resource fields

`RawMaterialDescription::Serialize` at `0x14012e9f0` stores the display key at
`+0x30`; integers at `+0x40` and `+0x44`; unit weight at `+0x48`; and category
flags at `+0x5c`. The storage and production callbacks use `+0x48` as unit
weight. `CreateRawMaterial` (`0x1400cc010`) selects one of `+0x40` or `+0x44`
based on an owner-field comparison, then multiplies by its action float at
`+0x4c`. The two stored quantities below are therefore reported as raw data
fields before that action multiplier; they are not asserted to be final yields
per work cycle.

| Workplace data resource | Inputs → output | Output label / template | Raw material `+0x40` / `+0x44` | Unit weight `+0x48` | Group |
| --- | --- | --- | ---: | ---: | ---: |
| `CutterYard` | 1 Wood → Firewood | Firewood | 3 / 4 | 4 | 3 |
| `IronMine` | none → Iron | Iron | 1 / 2 | 25 | 2 |
| `IronMine` | none → Coal | Coal | 3 / 4 | 15 | 9 |
| `Quarry` | none → Stone | Stone | 1 / 2 | 15 | 1 |
| `Tailor` | 2 Leather → LeatherCoat | Hide Coat [Leather] | 1 / 2 | 10 | 7 |
| `Tailor` | 2 Wool → WoolCoat | Wool Coat [Wool] | 1 / 2 | 10 | 7 |
| `Tailor` | 2 Wool + 2 Leather → WinterCoat | Warm Coat [Leather + Wool] | 1 / 2 | 10 | 7 |
| `Tavern` | 30 Apple → Ale | Ale [Apple] | 7 / 10 | 1 | 8 |
| `Tavern` | 60 Blueberry → Ale | Ale [Berries] | 7 / 10 | 1 | 8 |
| `Tavern` | 30 Cherry → Ale | Ale [Cherry] | 7 / 10 | 1 | 8 |
| `Tavern` | 30 Peach → Ale | Ale [Peach] | 7 / 10 | 1 | 8 |
| `Tavern` | 30 Pear → Ale | Ale [Pear] | 7 / 10 | 1 | 8 |
| `Tavern` | 30 Plum → Ale | Ale [Plum] | 7 / 10 | 1 | 8 |
| `Tavern` | 100 Wheat → Ale | Ale [Wheat] | 7 / 10 | 1 | 8 |
| `ToolMaker` | 1 Iron + 1 Wood → Tool | Iron Tool [Log + Iron] | 1 / 2 | 10 | 4 |
| `ToolMaker` | 1 Coal + 1 Iron + 1 Wood → SteelTool | Steel Tool [Log + Iron + Coal] | 1 / 2 | 10 | 4 |

“None” means the serialized alternative contains an empty input array. It does
not claim that the workplace has no other operating costs or constraints.

## Description-level fields retained without names

The enclosing descriptions also contain serialized numeric fields that should
remain identified by offset until their consumers establish the semantics:

| Resource | `+0x40` | `+0x50` | Type at `+0x38` | `+0x3c` | Float `+0x44` | `+0x48` | `+0x4c` |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| CutterYard | 1 | 0 | 0 | 0 | 4.0 | -1 | 0 |
| IronMine | 60 | 1 | 0 | 0 | 4.0 | 2000 | 0 |
| Quarry | 10 | 1 | 3861323576 | 1 | 10.0 | 2000 | 0 |
| Tailor | 5 | 11 | 0 | 0 | 4.0 | -1 | 0 |
| Tavern | 10 | 2 | 0 | 0 | 4.0 | -1 | 0 |
| ToolMaker | 5 | 6 | 0 | 0 | 4.0 | -1 | 0 |

The values above are read in serializer order and mapped back to object offsets;
the fourth column is the `Object::Type` field. The label-key column of the recipe
records and these enclosing fields are distinct.

## Runtime selection limits

The catalog records available alternatives, not a complete simulation policy.
`0x14011e3c0` initializes each alternative's component weight to `1.0`, and
`0x14011e940` samples the cumulative weights when the active selector is `-1`;
with those initialized weights unchanged, the alternatives have equal probability.
A nonnegative selector chooses that index directly. Other negative modes,
building UI controls that write the selector, and manager group limit meanings
remain untraced. The work callback and action lifecycle are described in
[RESOURCE-PRODUCTION-FLOW.md](RESOURCE-PRODUCTION-FLOW.md).

This is a static decode of shipped data and code. No production session was run,
and no final per-cycle output rate is claimed.
