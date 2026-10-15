# Event log and stock warnings

Slice F: the town event log (`Game +0x138`), the stock-manager warnings that post to it, and
the Event Log utility window. Port: `crates/game/src/events.rs`, `apps/game/src/gameplay.rs`
(posting), `apps/game/src/utility.rs` (`EventLogUi`, `format_event`). Evidence: Ghidra
pseudocode of the Steam x64 executable and the event logs in five current Steam saves.

## Log (`Game +0x138`)

| Offset | Field |
|---|---|
| `+0x00` | `Dialog/StringTable.rsc:events` |
| `+0x08` | `Dialog/StringTable.rsc:seasons` |
| `+0x10..+0x15` | six notify flags (event categories shown as popups) |
| `+0x18/+0x20` | entries (0x28 bytes each) |

`FUN_14009a2d0(log, type, severity, entity, @2, @3, @4)` adds an entry:

- text: entry `type` of `:events` with `@0` the season name (month index into `:seasons`),
  `@1` the 1-based year, `@2..@4` the arguments;
- colour by severity: 1 `ffffff00`, 2 `ff8080ff`, 3 `ffff6060`, 4 `ffea7517`, else white;
- cell: `FUN_14009a500`, the entity's map-rectangle centre, or -1;
- category: `FUN_14009a5a0` (0–7 → 0, 11 → 1, 12 → 2, 13 → 5, 14 → 8, 17–22 → 6, 23–24 → 7,
  25 → 9, 26/28 → 10, 27/29 → 11, 30 → 4, 31 → 3, else -1);
- icon 12 for types 26/28 and 13 for 27/29, else -1;
- types 11, 12, 30, 31 request AutoSlow when that option is on.

The port stores the format arguments and formats the text on display. The original stores
the formatted string. The two produce the same text, because the string tables do not change
during a game.

## Saved form

The block is `aaaaaaaa | size | version (1) | six notify bytes | count | entries | bbbbbbbb`.
Each entry is `u32 colour | string | type | severity | cell | category | icon`. Each string is
`len | 0x80000000` followed by the bytes, and `len` counts the terminating NUL.
`Save::event_log` (`crates/formats/src/save.rs`) finds the block by scanning for markers that
parse as exactly this and fill the block.

## Stock warnings

`FUN_140091780` (town step) recounts with `FUN_14008f3a0`, then runs `FUN_1400915d0`.

- Slots (`+0x170`): count, previous count, limit `+8`, consumption `+0x10`, cooldown `+0x18`.
- Low reserve: for slots 0–5 (types 17–22), the warning fires when `count < limit/20` and
  `limit/20 <= previous`. Firewood (slot 3) adds coal (slot 9) to all three values.
- Near capacity: produced goods (type 23) and logs/stone/iron (type 24, material flags
  `0x1c40`) fire when `used > (capacity/20)*19`. A storage's capacity counts towards a kind
  when it holds that kind.
- Each warning has a 120 s cooldown (`DAT_140178910`) and posts at severity 3.
- Default limits (`FUN_140095f00`): 200, 200, 200, 200, 50, 5000, 100, 50, 500, 200, 200, then
  1000.

## Event Log window

`EventLogUI` (`0xb07592fd`) shows the entries newest first in a display table. Column 0 is
the go-to button and column 1 the text. Buttons 100/101/102 scroll to the previous page, the
next page and the end. Rows set their colour from the entry.

## Verification

- `crates/game/tests/event_log_saves.rs` parses the logs of the local Steam saves
  (`save-game/`, never committed): 423 entries. Every colour, category and icon matches the
  port. For all 273 entries whose text takes no arguments (stock warnings, tornado, fire),
  the text is "season year: body" with a valid season and year ≥ 1. Two saves have empty logs.
- `tools/triage/determinism.sh 22 1234` stays identical. The log takes no Random.

## Pending

- EventNotify / EventPopup dialogs (`0x140068590`, `0x140069580`) and the notify checkboxes.
- Go-to buttons (`FUN_140067cd0`, camera to the entry's cell).
- Event sources other than stock warnings (births, deaths, coming of age, disasters) post
  as their systems are ported.
