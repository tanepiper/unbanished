# Event log and stock warnings

Slice F: the town event log (`Game +0x138`), the stock-manager warnings that post to it, and
the Event Log utility window. Port: `crates/game/src/events.rs`, `apps/game/src/gameplay.rs`
(posting), `apps/game/src/ui/utility.rs` (`EventLogUi`, `format_event`). Evidence: Ghidra
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

## Event Log checkboxes and go-to

OnOpened (`FUN_140067fa0`) gives the `notifybuttons` children ids `0x67 + i` and checks them
from the log's flags. The children are, in order: old-age deaths, deaths, disasters, storage
and town. A click (`FUN_140067c80`) copies the checkbox into flag `i`. Flag 5 ("other") has no
checkbox. Every inspected save holds `0 1 1 1 1 1`, which the port uses as its default.
Row buttons are `0x6c + r`. `FUN_140067cd0` takes entry `n - scroll - rows + r` and, when
its cell is set, moves the camera target to the cell centre (`x = cell % w - (w/2 - 0.5)`,
the same for `y`).

## Notification ribbon (`Dialog/EventNotify.rsc:resource`)

`FUN_14009d0d0` adds the toolbar, EventNotify and AutoSave dialogs in the same frame.
`UserInterface::AddDialog` queues each dialog, and `UserInterface::Update` appends the queue
to the draw list in reverse order. All three dialogs are on draw layer 0 (Dialog `+0x34`), so
the toolbar draws over the ribbon. `Screen` mirrors this: dialogs added in one frame are
inserted before one another. Update also lays out every dialog each frame, before the loop
runs.

- OnOpened `FUN_140069030` binds the six `notifyContent` slots (`notifyImage`,
  `notifyButton` with id `0x5dc + i`) and stores the entry count as the last entry seen.
- Update `FUN_140068590`:
  - Notifications age by the frame time and go when under one tick.
  - Each new entry adds a notification when its severity is above 0, its group's notify
    flag is set and its category is set. Groups: type 2 → 0; 0, 1, 3–7 → 1;
    11, 12, 30, 31 → 2; 17–24 → 3; 13, 14 → 4; else 5.
  - A notification holds a 30 s lifetime, sprite `EventIcon{category}`, the severity colour,
    and the text after "season year: ".
  - An entry with an icon also opens an EventPopup.
  - The oldest notifications go when there are more than the slots.
  - When anything was added, `Audio/UI/WarningEffect.rsc` (global sound slot `0x13`) plays.
  - The update then stores `count - 1`, so an entry added after the ribbon opens but before
    its first update is never shown.
- Refresh `FUN_140068a20`:
  - The dialog root's right padding eases towards `toolbar width - own width + 44 + 40·(n-1)`
    (0 with no notifications), at 0.8 old and 0.2 new. Here `+0x60` is
    `Element::GetLayoutWidth` and `SetPadding` takes left, right, top, bottom. The ribbon
    therefore slides out from behind the toolbar, one 40 px slot per notification.
  - An entry with a cell shows the button, which moves the camera there (`FUN_140068f00`).
    One without a cell shows the plain image.
  - Each slot's tooltip is the notification text.
- EventPopup (`FUN_140069580`): `imageIcon` shows `EventIcon{icon}`, `textEvent` the full
  text, and `buttonOk` (id 200) closes the popup (`FUN_140069530`).

## Verification

- `crates/game/tests/event_log_saves.rs` parses the logs of the local Steam saves
  (`save-game/`, never committed): 423 entries. Every colour, category and icon matches the
  port. For all 273 entries whose text takes no arguments (stock warnings, tornado, fire),
  the text is "season year: body" with a valid season and year ≥ 1. Two saves have empty logs.
- `apps/game` test `event_ribbon_popup_and_goto` checks, with the shipped dialogs:
  - slot images and buttons by cell;
  - the tooltip text;
  - the popup and its OK button;
  - the camera go-to;
  - an unchanged Random.
- Capture: `UNBANISHED_SEED=1234 unbanished --boot-capture OUT 18 --press buttonNew@4.5
  --press buttonOk@5.0 --event 22@12 --event 31:20000@12.5 --event 30@13` shows three
  notifications sliding out left of the toolbar. `--event KIND[:CELL]@SECONDS` posts an event
  at severity 3. Type 26 opens the "scenario completed" popup.
- `tools/triage/determinism.sh 22 1234` stays identical. The log takes no Random.

## Pending

- The alert sound: `Audio/UI/WarningEffect.rsc` is a SoundEffect, which is not parsed yet, and `Audio::Scene` is not ported. The 64-channel Device can already play it alongside the music.
- The camera go-to keeps the port's camera height (the original uses the ground point).
- Not compared against an original screenshot of the ribbon yet.
- Event sources other than stock warnings (births, deaths, coming of age, disasters) post
  as their systems are ported.
