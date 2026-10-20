# Utility windows (Status Bar, Event Log, Professions, …)

This note covers how the toolbar's "reports" entries open their windows and how the Status Bar
fills its values. The evidence is Ghidra pseudocode of the Steam x64 build (1.0.7), the Runtime DLL
and the shipped `Template/*_ui.crs` resources. The port is `apps/game/src/ui/utility.rs`. The parser is
`unbanished_formats::game::ui_description`; all 39 shipped `UIDescription` resources parse
(`crates/formats/tests/toolbar.rs`). Earlier traces are in
[GAME-LOOP-STEP-07.md](GAME-LOOP-STEP-07.md) and [GAME-LOOP-STEP-10.md](GAME-LOOP-STEP-10.md).

## `UIDescription` (serializer `FUN_140133350`)

| Field | Content |
|---|---|
| `+0x28` | host dialog reference, e.g. `Dialog\StandardDialog.rsc:resource` (`:resourceUserTitle` for the Status Bar, `:entity` for buildings) |
| `+0x40` | int (0 in every utility) |
| `+0x30` | controller table, 0x28 bytes each (`FUN_140133530`): `u32` controller type id, element reference, config reference, tab reference, slot name |
| `+0x20` | int (512 for Help, otherwise 0) |

Utility controllers include:

| Window | Controller | Content |
|---|---|---|
| Status Bar | `0x509d1aea` `StatusBarUI` | `Dialog\StatusBar.rsc:content` in `userGroup0`, plus `groupEffects` in `userTitle0` |
| Event Log | `0xb07592fd` | `Dialog\EventLog.rsc:content` in `userGroup0`, plus `notifybuttons` in `userTitle0` |
| Professions | `0xc8418bd4` | `Dialog\Profession.rsc:content` |
| Map | `0x42c5355e` | `Dialog\Map.rsc:content` |

## Opening a window (`FUN_140106f50`)

The toolbar's kind-2 branch passes the item's template:

1. If a dialog for the template is open, `UserInterface::CloseDialog` closes it, and the remembered
   entry's "open" byte is cleared.
2. Otherwise it finds the template's `UIDescription` child and builds a `UI::DialogInstance` from
   it. The Runtime constructor runs `FUN_1401335d0` for each table entry:
   - **slot is a `TabElement`:** `AddTab` with the tab and element references;
   - **otherwise:** `DialogInstance::AddController`, which instantiates the element into the slot
     and attaches the controller found through `Object::Factory`. Type 0 attaches nothing.
3. The window's upper-left corner comes from a map on the gameplay object (`+0x50`) keyed by
   template: `{x, y, open}`. A new entry starts at (0, 0) and open.
   `DialogInstance::SetUpperLeft` clamps the corner to the screen.
4. `UserInterface::AddDialog`.

Dragging (ToolUI `FUN_140081100`, `FUN_140080e80`, `FUN_140080e10`). A window with a
`borderTitle` has ToolUI `+0x40` set, so a press on the title makes the window follow the
cursor at once. Its corner is kept at 0 or more and written back to the `+0x50` entry, so it
reopens where it was left (`gameplay::drag_window`).

The toolbar is the only caller.

Entity selection uses the sibling entry point `FUN_140102880` to construct the same description
with an entity owner, `:entity` host, tab attachments and active page. See
[SELECTION.md](SELECTION.md) for slice B and its verification/limits.

## Status Bar (`StatusBarUI`)

`FUN_14007f8a0` (OnOpened) binds named elements:

- `textSeason`, `textPopulation`, `numberYear`, `numberTemperature`;
- `numberWood`, `Stone`, `Iron`, `FireWood`, `Tools`, `Food`, `Herbs`, `Clothes`, `Alcohol`,
  `Coal`, `Textile`, and `numberCustom0..9`;
- six status icons, `imageStatus{Home,Food,Cold,Job,Tool,Disease}`, which get element ids
  100–105, with matching `numberStatus*` counts;
- the five weather images;
- `groupHearts` and `groupStars`;
- `labelTitleParam`. Its parameter 0 is the town name.

`FUN_14007eda0` (OnUpdateLayout) does the following:

- **Population:** it loops over the citizens.
  - Age `+0x3c` below the adult age (description `+0x44`) means a child, unless the citizen has a
    school at `+0x208`, in which case it counts as a student. Everyone else is an adult.
  - `textPopulation` takes `@0` adults, `@1` students, `@2` children.
- **Status:** it gets each citizen owner's StatusIconComponent and counts its bits (`+0xf0`): 1 home, 2 food, 4 cold, 8 job,
  16 tool, 32 disease. An icon and its count are hidden while no citizen has that bit.
- **Hearts and stars:** these show the average health (`+0x50`) and happiness (`+0x80`), through
  `FUN_14006b830`, with the citizen description's scale.
- **Year and season:** `numberYear` shows the calendar year, and `textSeason` shows
  `seasons:Month{n}`.
- **Temperature:** it reads the environment controller's temperature (`+0x18`, °F). It is
  converted to °C if "Show Celsius" is set, then truncated.
- **Weather:** the weather image is the environment state (`+0x10`). State 0 at 32° or below
  becomes 4 (Snow). The images are indexed Rain, Cloudy, PartlySunny, Sunny, Snow.
- **Stock:** it reads the stock manager's inventory (`+0x170`, 0x20 bytes per category) in the
  order wood, stone, iron, firewood, tools, food, herbs, clothes, alcohol, coal, textile, then the
  custom slots `0x160..0x2a0`.

`UI::Element::SetFlagsOnOff(on, off)` with flag 1 is how elements are hidden and shown.

## Live Status Bar slice (2026-10-15)

Static x64 evidence, supplemented by original-function emulation:

- `FUN_14007eda0` walks citizens in manager creation-list order. It counts adults, students and
  children, sums six StatusIconComponent flags and averages raw health/happiness with f32
  additions in that same order. The first citizen description supplies the health scale `+0x70`
  and happiness scale `+0xa4`; the empty-town scale is 1. Stock already comes from the live
  storage ledgers and retains stock-slot order.
- Correction: `+0xf0` belongs to **StatusIconComponent**, type id `0x7f050e80` (registration
  `FUN_140155910`, runtime record `DAT_1401cfe08`), reached through Citizen `+0x280`. It is
  separate from Citizen `+0x88` state/event flags. `FUN_1401318c0` sets/clears one mask bit;
  additions are suppressed for disabled owners, removals always work. Tool failure sets bit 4;
  tool message 1 (`FUN_1401152b0`) clears it. This is not a SelectableComponent overlay.
- Citizen update `FUN_140118340` sets home bit 0 when no home exists; food bit 1 above
  `D+0xcc + (D+0xd0 - D+0xcc)*0.5`; cold bit 2 above
  `D+0xec + (D+0xf0 - D+0xec)*0.25`; job bit 3 when profession byte `+0x4c` is false and
  work `+0x248` is absent; disease bit 5 while `+0x68` is present. These writes are simulation
  work. The UI only reads the resulting mask. The existing per-iteration citizen-driver
  scheduling deviation remains; needs/assignment updates themselves are still incomplete.
- `FUN_14006b550` binds `whole` and `half` children inside each packaged meter slot.
  `FUN_14006b830` truncates `(value/scale)*(slotCount*2)+0.5`. For odd boundaries 1,3,5,…,
  whole is visible below the truncated result, half is visible at equality. Containers retain
  their resource geometry; hiding entire containers incorrectly changes the layout.
- Status snapshots live in `crates/game/src/people/status.rs`; citizen health/happiness are live state
  fields initialized from `FUN_140119520`. UI open/close/layout/hover/draw accept no mutable
  simulation state or game Random. Utility positions use BTreeMap, open windows retain Vec order.

`tools/emu/ui_status.py` produces 400 meters, 400 mask mutations and 200 complete original
StatusBarUI aggregations, compared bit-for-bit by `crates/game/tests/ui_status_emu.rs`. UI writes,
string construction, object identity and scene notifications are stubbed; aggregation and numeric
logic execute in the original binary. An integration test repeatedly opens/closes, lays out,
hovers and draws the packaged Status Bar and proves unchanged Random, simulation time and
citizen state, including a visible half-heart at 50% health.

The Status Bar opens with `--press reports@10 --click-at 474,713@10.3` at 1280×800.
`tools/triage/determinism.sh` now accepts additional capture arguments after seconds/seed and
fails if a named scripted control is missing. Use the same click sequence to verify this window.
Opening the window exposed that town-name generation used the independent clock-seeded menu
stream. The explicit `UNBANISHED_SEED` capture override now fixes that stream too; normal runs
retain clock seeding. Opening/rendering the window never draws from either stream.

Validation: `cargo test --workspace` passes 103 tests (two existing save tests ignored), and
the wasm32 release build passes. Both the baseline and Status Bar scripted captures are
identical across two seeded runs. Local visual capture:
`game-data/derived/captures/ui-status-22.png` (1280×800, seed 1234, 22 seconds).

## Port status

- Status Bar: live population, stock, status counts, health/happiness scales, town name, calendar,
  environment temperature and weather; packaged whole/half hearts and stars.
- Professions: ordered packaged rows, live counts per profession and workplace places
  (`numberAvailable`); spinner changes (event `0x15`, ids `300 + i`, clamped to 0..count +
  laborers) become `SetCount` commands applied before the next iteration (`FUN_14007b890`, see
  [JOBS.md](JOBS.md)). The goto buttons (`500 + i`, `FUN_14007bf00`): with any working adult
  (grown, `+0x3c` ≥ the adult age, not at school `+0x208`), the next citizen after the last one
  shown (`+0x70`) in the town's list with the profession (`+0x1f0`); after as many misses as
  there are working adults, whoever was reached. The camera centres on it and it is selected
  (`FUN_140108510`).
- Help (`HelpUI`, vtable `0x140162070`: bind `FUN_14006ae60`, refresh `FUN_14006ab90`, events
  `FUN_14006abf0`): `comboTopic` (`0xe74`) lists the pages of `Game\Help\HelpTopics.rsc`
  (`Game +0xa0`, string tables of Topic, Title and Text) by title (`StringTable::GetText(1)`);
  the chosen page's text (2) fills `documentHelp` (`0xe75`), scrolled to the top, and joins
  the history after the current page (`FUN_14006a880`). A link opens the page whose topic (0)
  it names (`FUN_14006aaa0`); back (`0xe76`) and forward (`0xe77`) walk the history, each page
  keeping its scroll; they are enabled when there is somewhere to go.
- Utility windows: open/close through UIDescription and StandardDialog; remembered positions.
- Entity hosts: click picking, selection outline, localized titles, packaged tabs, pin/close;
  live building/citizen controller refresh is pending.
- Pending: other controllers, dragging/persistence, status billboards, Map,
  event sources/notifications, full tooltip behavior. Unsupported controller ids are logged.
