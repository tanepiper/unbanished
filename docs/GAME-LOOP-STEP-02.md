# Game loop step 02: camera input, entity scheduling, and gameplay UI startup

This step follows three edges from the `Game/Game` outer update: its per-frame input controller,
the entity manager tick, and the UI installed on entry to gameplay. Evidence is static Ghidra
12.1.4 pseudocode for the Steam x64 executable, plus existing entity lifecycle tracing. No live
Banished session was observed in this pass. The current REA Ghidra provider is unconfigured
(`GHIDRA_INSTALL_DIR` is unset), so this pass uses the repository's existing Ghidra exports and
does not claim a fresh REA analysis session or Evidence ID.

## Per-frame controller is also the camera input path

The controller at `Game/Game +0x120` participates in both simulation gating and camera handling.
Its byte at `+0x30` is toggled by `FUN_140104bb0` when input code `0x17` is pressed (unless a global
configuration bit blocks it). The outer game update reads its integer at `+0x34` as the number of
simulation iterations when that byte is clear.

`FUN_140104390` reads key-map values and updates the scene camera transform at `Game/Game +0xe0`.
It handles movement input, optional screen-edge scrolling, camera rotation/height state, and
clamps the camera against the generated map. `FUN_140103da0` applies one of five saved camera
presets from the controller's table at `+0x70`; the shortcuts reach it from `FUN_140104bb0`. The
registered action IDs now have names from `FUN_14009e7c0`; see
[KEYMAP-AND-CAMERA-CONTROLS.md](KEYMAP-AND-CAMERA-CONTROLS.md). Its final `KeyMap::Load` call means
the registered device/code pairs are defaults that user settings can override.

This is a second reason the HUD must not swallow unused keys: Banished handles camera input in the
game-loop update path even while the in-game toolbar is present. The port's adapter now forwards
unhandled movement keys and pointer camera gestures; see [CAMERA-INPUT-FINDING.md](CAMERA-INPUT-FINDING.md).

## Entity manager tick

Each simulation iteration calls `FUN_1400c9560` with the same frame delta. The manager adds that
delta to its own elapsed time, updates its active bit-`0x04` set, then services five bucket arrays
with per-bucket cursors and quotas. For a due entity it calculates the elapsed time since that
entity's timestamp, calls component virtual slot `+0x48` for components carrying update bit `0x08`,
then stores the new entity timestamp. The detailed insertion, scheduling and component rules are
in [ENTITY-COMPONENT-LIFECYCLE.md](ENTITY-COMPONENT-LIFECYCLE.md).

This establishes the entity update cadence mechanism, but not its configured quotas, real-time
interval, calendar mapping, or exact ordering between every gameplay subsystem. The game-level
calendar transition is still open.

## Gameplay UI installation

`FUN_14009d0d0` is the handoff after the world/managers have been initialized. It enables the
cursor, loads the toolbar description and adds three separate UI dialogs:

| Resource | Constructor path | Observed setup |
|---|---|---|
| `Dialog/Toolbar.rsc` | `FUN_1400bede0` | Finds `toolbarContent` and `toolBar`; the toolBar element receives ID 1000. It builds children from registered toolbar-item resources, assigns their IDs, user data and tooltips, and applies icon sprites. |
| `Dialog/EventNotify.rsc` | `FUN_140069410` | Created as a separate dialog instance with its own controller. |
| `Dialog/AutoSave.rsc` | `FUN_14005eb90` | Binds `savingImage` and `animatedImage` elements. |

After adding the dialogs, the game fades in from black and calls `FUN_1401073b0` to initialize the
controller's registered state. This proves that the original gameplay UI is built from packaged
dialog resources and a dynamically populated toolbar. It does not yet identify each toolbar action
or establish the full event-notification/autosave behavior.

The shipped [Status Bar tree and controller](GAME-LOOP-STEP-06.md) are traced through its
`StatusBarUI` bindings and `OnUpdateLayout` callback in [step 07](GAME-LOOP-STEP-07.md). The
gameplay bootstrap adds the Toolbar dialog, and the package graph references `UtilityStatusBar`, but
the direct resource handoff from Toolbar to that utility is still unproven.

## Evidence pointers and next trace

- `FUN_14009b850` (`0x14009b850`): loop ordering and controller iteration count.
- `FUN_140104bb0` (`0x140104bb0`): pause toggle and camera-preset shortcut dispatch.
- `FUN_140104390` (`0x140104390`): per-frame camera movement/transform update.
- `FUN_140103da0` (`0x140103da0`): camera-preset application.
- `FUN_1400c9560` (`0x1400c9560`): elapsed-time entity scheduler.
- `FUN_14009d0d0` (`0x14009d0d0`): gameplay UI installation.
- `FUN_1400bede0` (`0x1400bede0`): toolbar element construction.
- `FUN_140069410` and `FUN_14005eb90`: event notification and autosave dialog constructors.

The toolbar action trace is now recorded in [GAMEPLAY-SPEED-AND-TOOLBAR.md](GAMEPLAY-SPEED-AND-TOOLBAR.md):
the packaged values are 1, 2, 5 and 10 iterations per frame, and the toolbar buttons update the
controller fields described above. The calendar accumulator and month/year transition rules are
now traced in [GAME-LOOP-STEP-03.md](GAME-LOOP-STEP-03.md). The weather state and climate update are
in [GAME-LOOP-STEP-04.md](GAME-LOOP-STEP-04.md), [GAME-LOOP-STEP-05.md](GAME-LOOP-STEP-05.md) and
[GAME-LOOP-STEP-06.md](GAME-LOOP-STEP-06.md). Continue with the Toolbar/`UtilityStatusBar` resource
handoff and trace how elapsed time reaches simulation components.
