# Game loop step 07: status bar bindings and live values

This step follows the shipped Status Bar resource tree into its controller constructor and periodic
update. Evidence is static Ghidra 12.1.4 decompilation of the Steam x64 executable and decoded
compiled UI resources. It identifies the update logic, but not the controller's full creation and
registration lifecycle or a live game's rendered values.

## Controller bindings

`FUN_14007f8a0` binds the `Dialog/StatusBar.rsc` elements to controller fields. The bindings include
season, population and year; temperature and five weather images; the home, food, cold, job, tool
and disease status counts; and the resource inventory numbers, including the custom resource slots.
The weather image pointers are stored in this order:

| Controller offset | Dialog element |
|---:|---|
| `+0x1a8` | `imageRain` |
| `+0x1b0` | `imageCloudy` |
| `+0x1b8` | `imagePartlySunny` |
| `+0x1c0` | `imageSunny` |
| `+0x1c8` | `imageSnow` |

This connects the static resource tree described in [step 06](GAME-LOOP-STEP-06.md) to a concrete
controller. It also shows that `FUN_14009d0d0`'s toolbar/event/autosave installation is not the only
gameplay UI setup path.

The executable's type metadata identifies the controller as `StatusBarUI`, derived from
`UI::DialogController`. `FUN_140152640` initializes that type record; its factory allocates a
`0x1d0`-byte instance and the object constructor installs the matching vtable and clears its fields.
The type record and object constructor are distinct from `FUN_14007f8a0`, which binds controls from
the dialog instance.

## Periodic update

`FUN_14007eda0` reads the game calendar and state counters, then writes the bound `NumberElement`
and `TextElement` values. It uses the current year and elapsed game time to select `Month0` through
`Month11` from the season string table. It reads temperature from the environment controller at
`+0x18`, and the weather state from `+0x10`. If the global setting byte at `DAT_1401cbde0 + 0x15`
is set, the displayed temperature is converted with `(F - 32) * 5 / 9` before truncation to an
integer. The decoded `Options_checkShowCelsius` and `Options_textShowCelsius` resources label that
setting as “Show Celsius.”

For icon selection, the update starts with the environment weather-state index. When the displayed
temperature is at most 32 and that state is zero, it substitutes index 4. It then iterates the five
image pointers in constructor order and calls `UI::Element::SetFlagsOnOff` with complementary
booleans for each index. This yields the observed index choice: state 0 selects Rain above the cold
threshold and Snow at or below it; indices 1, 2, and 3 align with Cloudy, Partly Sunny, and Sunny;
the explicit cold fallback selects Snow. The update routine does not show a separate mapping for
weather-state indices beyond the five images.

The temperature conversion happens before the `<= 32` fallback check in the decompiled routine.
Consequently, when Celsius display is enabled, this check compares the converted value to 32. That
ordering is directly visible in the code; its effect on an actual game display has not been observed.

## What is still open

The type identity and factory are now known, but the current reference export does not identify the
call site that creates and schedules this controller or connects it to `Dialog/StatusBar.rsc`. The
values behind all resource counters and each status category also need to be traced to their owning
managers before reproducing them. The current port still uses its compact HUD and does not render
this original status bar.

## Evidence pointers

- `FUN_14007f8a0` (`0x14007f8a0`): binds the Status Bar controls and stores the five weather images.
- `FUN_14007eda0` (`0x14007eda0`): refreshes calendar, temperature, weather icon and HUD counters.
- `FUN_140152640` (`0x140152640`): registers type metadata for `StatusBarUI` with base
  `UI::DialogController`; `FUN_140014af0` is its allocation factory and `FUN_1400803f0` initializes
  the instance fields and vtable.
- `FUN_140099430` / `FUN_140099160`: environment controller defaults and serialized weather state,
  described in [step 06](GAME-LOOP-STEP-06.md).
- `Dialog/StatusBar_content`, `Dialog/Options_checkShowCelsius.crs`, and
  `Dialog/Options_textShowCelsius.crs`, decoded with `tools/crs/types/ui.py`.
- Local exports: `analysis/gameplay-statusbar-controller.txt`,
  `analysis/gameplay-statusbar-references.txt`, and `analysis/statusbar-address-references.txt`.
  These Ghidra pseudocode exports remain local; this note records the findings needed in source
  control.

## Next trace

Identify the class and lifecycle that owns the Status Bar controller. Then follow each displayed
counter to the game manager that supplies it, keeping resource inventory, citizen status counts and
global settings as separate data sources.
