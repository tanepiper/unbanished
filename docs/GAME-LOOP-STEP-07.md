# Game loop step 07: status bar bindings and live values

This step follows the shipped Status Bar resource tree into its controller constructor and layout
refresh callback. Evidence is static Ghidra 12.1.4 decompilation of the Steam x64 executable and
Runtime DLL plus decoded compiled UI resources. The status-bar item's click dispatch and
`StatusBarUI` controller attachment are traced in [step 10](GAME-LOOP-STEP-10.md); live visibility
and rendered values remain unverified.

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
controller. `FUN_14009d0d0` adds the top-level Toolbar, EventNotify, and AutoSave dialogs. The
status-bar Toolbar item is serialized with action 2 and references this utility resource; the native
action-2 handler creates the dialog, and the UIDescription controller table instantiates
`StatusBarUI`. See [step 10](GAME-LOOP-STEP-10.md) for evidence.

The executable's type metadata identifies the controller as `StatusBarUI`, derived from
`UI::DialogController`. `FUN_140152640` initializes that type record; its factory allocates a
`0x1d0`-byte instance and the object constructor installs the matching vtable and clears its fields.
The type record and object constructor are distinct from `FUN_14007f8a0`, which binds controls from
the dialog instance.

## Packaged utility route

The compiled `Game/Toolbar_resource.crs` enumerates a `statusbar` item, and
`Game/Toolbar_statusbar.crs` references `Template/UtilityStatusBar.rsc:resource`. The utility resource
then points to `ui` and `toolbar`; its UI references `Dialog/StatusBar.rsc:content` and
`Dialog/StatusBar.rsc:groupEffects`, and its toolbar description names the `ToolStatusBar` icon and
tooltip. This establishes the packaged resource route from the game's toolbar to the Status Bar UI.
The code that handles activation and creates/registers the `StatusBarUI` controller from this route
has not been identified.

## Layout refresh callback

`FUN_14007eda0` is not proven to be a per-frame update. The StatusBar vtable at `0x140163c40` puts
it in slot 7 (`+0x38`), which the Runtime `DialogController` vtable identifies as
`OnUpdateLayout`. At each UI update, `UI::UserInterface::Update` traverses registered dialogs;
`DialogInstance::OnUpdateLayout` walks its controller list and dispatches that slot. Thus the status
refresh is tied to dialog layout updates. The available static evidence does not establish the
frequency of those layout updates in a live game.

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

The controller's Runtime creation and callback lifecycle is known: `DialogInstance::AddController`
resolves the type through `Object::Factory`, creates it, supplies its dialog/element/config context,
calls `OnCreated`, and registers it. The gameplay bootstrap adds the Toolbar dialog, and the
compiled resource route reaches `Dialog/StatusBar.rsc`. The statusbar item's action-2 Toolbar
activation, dialog add/close path, and `StatusBarUI` attachment are traced in
[step 10](GAME-LOOP-STEP-10.md). The values are traced in
[step 08](GAME-LOOP-STEP-08.md); the code that sets citizen status flags and updates stock quantities
remains open. The current port uses its compact HUD and does not render this original status bar.

## Evidence pointers

- `FUN_14007f8a0` (`0x14007f8a0`): binds the Status Bar controls and stores the five weather images.
- `FUN_14007eda0` (`0x14007eda0`): refreshes calendar, temperature, weather icon and HUD counters.
- `FUN_140152640` (`0x140152640`): registers type metadata for `StatusBarUI` with base
  `UI::DialogController`; `FUN_140014af0` is its allocation factory and `FUN_1400803f0` initializes
  the instance fields and vtable.
- StatusBar vtable `0x140163c40`: slot 5 (`+0x28`) is `FUN_14007f8a0` (the `OnOpened`-position
  binding callback); slot 7 (`+0x38`) is `FUN_14007eda0` (`OnUpdateLayout`). Runtime
  `DialogInstance::AddController` creates/registers controllers; `DialogInstance::OnUpdateLayout`
  dispatches the layout callback.
- `FUN_14009d0d0` (`0x14009d0d0`) finds and adds `Dialog/Toolbar.rsc`, `Dialog/EventNotify.rsc`, and
  `Dialog/AutoSave.rsc` to the gameplay `UserInterface`.
- `FUN_140099430` / `FUN_140099160`: environment controller defaults and serialized weather state,
  described in [step 06](GAME-LOOP-STEP-06.md).
- `Dialog/StatusBar_content`, `Dialog/Options_checkShowCelsius.crs`, and
  `Dialog/Options_textShowCelsius.crs`, decoded with `tools/crs/types/ui.py`.
- `Template/UtilityStatusBar_resource.crs`, `Template/UtilityStatusBar_ui.crs`, and
  `Template/UtilityStatusBar_toolbar.crs`; packaged references connect them to the status bar
  content and `ToolStatusBar` button.
- `Game/Toolbar_resource.crs` and `Game/Toolbar_statusbar.crs`; the toolbar list includes a
  `statusbar` item that references the utility template.
- Local exports: `analysis/gameplay-statusbar-controller.txt`,
  `analysis/gameplay-statusbar-references.txt`, and `analysis/statusbar-address-references.txt`.
  These Ghidra pseudocode exports remain local; this note records the findings needed in source
  control.

## Next trace

Capture live visibility and values for the statusbar dialog. Continue from
[step 08](GAME-LOOP-STEP-08.md) into the concrete owners and update paths for citizen status and
resource stock values.
