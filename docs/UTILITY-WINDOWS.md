# Utility windows (Status Bar, Event Log, Professions, …)

This note covers how the toolbar's "reports" entries open their windows and how the Status Bar
fills its values. The evidence is Ghidra pseudocode of the Steam x64 build (1.0.7), the Runtime DLL
and the shipped `Template/*_ui.crs` resources. The port is `apps/game/src/utility.rs`. The parser is
`banished_formats::game::ui_description`; all 39 shipped `UIDescription` resources parse
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

The toolbar is the only caller.

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
- **Status:** it ORs and counts each citizen's status bits (`+0xf0`): 1 home, 2 food, 4 cold, 8 job,
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

## Port status

- **Done:**
  - Utility windows open and close from the toolbar, built from their `UIDescription` into the
    `StandardDialog` host. The title bar's close button also closes them.
  - The Status Bar controller is ported. Its values come from the gameplay loop: town name,
    calendar year and month, and temperature and weather.
  - Citizens and stock are reported as an empty town until those managers exist.
- **Not done:**
  - The other controllers (Event Log, Professions, Map, …) are logged and show their raw
    templates.
  - Window dragging and saving positions.
  - The hearts and stars scale.
  - Temperature and weather use the placeholders 50 °F and sunny until the environment
    controller is ported (see [GAME-LOOP-STEP-05.md](GAME-LOOP-STEP-05.md)).
