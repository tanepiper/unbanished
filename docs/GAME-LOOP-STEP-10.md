# Game loop step 10: Toolbar dialog activation

This step follows a Toolbar event into the dialog creation path. Evidence is static Ghidra 12.1.4
pseudocode for the Steam x64 executable plus the decoded packaged resource references. It does not
include a live click capture.

## Event to dialog

The Toolbar dialog callback `FUN_1400be770` receives an event kind and selected item index. For its
event kind `0x12`, it reads the selected Toolbar item from the dialog's item array and switches on
the item's action value at `+0x48`. Action 0 opens or closes a `ToolbarDialog` category popup. Action
2 calls `FUN_140106f50` with the gameplay controller and the selected item's referenced external
resource at `+0x58`.

`FUN_140106f50` walks that external's children for a `UIDescription`. If it finds one and there is
not already a matching open dialog, it constructs a `UI::DialogInstance` from the UI description,
passes the selected external as its associated resource, and adds the instance to the gameplay
`UserInterface`. If the matching dialog is already open, it closes it. This establishes a
resource-driven Toolbar-to-dialog activation mechanism in the native code.

## Status Bar connection and limit

The compiled `Game/Toolbar_statusbar.crs` record stores action value **2** at Toolbar item offset
`+0x48`, parent `Game/Toolbar.rsc:reports` at `+0x60`, and
`Template/UtilityStatusBar.rsc:resource` at `+0x58`. These are the exact value and field consumed
by `FUN_1400be770`'s action-2 branch, which passes `item +0x58` to `FUN_140106f50`. The status-bar
item therefore enters the resource-backed dialog path.

The resource chain is:

`Game/Toolbar.rsc:statusbar` -> `Game/Toolbar_statusbar.crs` ->
`Template/UtilityStatusBar.rsc` -> `ui` -> `Dialog/StatusBar.rsc:content`.

`FUN_140106f50` searches the selected UtilityStatusBar external for its `UIDescription`, creates a
`UI::DialogInstance` from that description while passing the selected utility resource as context,
and adds the dialog to the gameplay `UserInterface`. If that dialog is already open, the function
closes it. The utility template also names `toolbar`; its ToolbarDescription names the
`ToolStatusBar` icon and tooltip. `Template/UtilityStatusBar_ui.crs` is a `UIDescription` whose
controller table contains type ID `0x509d1aea`, the CRC32 type ID for `StatusBarUI`, alongside
`Dialog/StatusBar.rsc:content` and element name `userGroup0`. The Runtime
`UI::DialogInstance::DialogInstance` constructor iterates the `UIDescription` controller table and
calls `AddController`; `AddController` resolves the type through `Object::Factory`, constructs it,
and runs its setup path with the utility resource context. This closes the static controller
attachment path as well as the Toolbar click path.

The click, dialog add/close, and `StatusBarUI` attachment are established from static code and
resource bytes. A live game's visibility state and rendered values have not been captured.

## Relation to the port

The port currently renders a compact gameplay HUD and does not build the original Toolbar or
Status Bar dialog tree. The native activation and controller path are traced. Porting this UI still
depends on implementing its dialog bindings and update behavior.

## Evidence pointers

- `analysis/export/Application-steam-x64.exe/decomp.c`: `FUN_1400be770` (`0x1400be770`) dispatches
  Toolbar events; `FUN_140106f50` (`0x140106f50`) resolves a `UIDescription`, creates a dialog and
  adds it to the gameplay `UserInterface`.
- `Game/Toolbar_statusbar.crs`, `data0.pkg` offset `41,337,962`, class `Toolbar`: action `2`, parent
  `Game/Toolbar.rsc:reports`, related external `Template/UtilityStatusBar.rsc:resource`. Field order
  follows `FUN_1400d74e0` (`0x1400d74e0`) serialization.
- `Template/UtilityStatusBar_ui.crs`, class `UIDescription`: controller type `0x509d1aea` matches
  `StatusBarUI`, with the `Dialog/StatusBar.rsc:content` element binding. The application serializer
  is `FUN_140133350` (`0x140133350`). Runtime dialog construction and controller registration are
  in `analysis/export/Runtime-steam-x64.dll/decomp.c` at `UI::DialogInstance::DialogInstance`
  (`0x180117100`) and `AddController` (`0x180115d00`).
- `analysis/export/Application-steam-x64.exe/decomp.c`: type metadata at `0x1401cff90` names
  `UIDescription` derived from `Description`.
- `Game/Toolbar.rsc`, `Game/Toolbar_statusbar.crs`, and the `Template/UtilityStatusBar_*` compiled
  resources, inventoried in `docs/GAME-RESOURCE-INVENTORY.tsv` and
  `docs/SAVE-EXTERNAL-REFERENCES.tsv`.
- [GAME-LOOP-STEP-07.md](GAME-LOOP-STEP-07.md): StatusBar bindings, layout callback and packaged
  resource route.

## Next trace

Continue the new-game loop into the next simulation manager after calendar/environment updates,
recording each manager's input and output before porting its behavior.
