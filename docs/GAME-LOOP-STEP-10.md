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

The compiled resources form a matching candidate route:

`Game/Toolbar.rsc:statusbar` -> `Game/Toolbar_statusbar.crs` ->
`Template/UtilityStatusBar.rsc` -> `ui` -> `Dialog/StatusBar.rsc:content`.

The utility template also names `toolbar`; its ToolbarDescription names the `ToolStatusBar` icon
and tooltip. Combined with `FUN_140106f50`, this shows how a Toolbar item can resolve a packaged UI
description and display it. The available decoded resource record does not yet establish the
numeric action value at the Toolbar item's `+0x48`, so this trace does not claim that
`FUN_140106f50` is definitively the `statusbar` item's click branch. Nor does it establish a live
session's visibility state.

## Relation to the port

The port currently renders a compact gameplay HUD and does not build the original Toolbar or
Status Bar dialog tree. The native activation path is now partially traced, but porting this UI still
depends on decoding Toolbar action metadata and following the utility template's controller
creation/configuration.

## Evidence pointers

- `analysis/export/Application-steam-x64.exe/decomp.c`: `FUN_1400be770` (`0x1400be770`) dispatches
  Toolbar events; `FUN_140106f50` (`0x140106f50`) resolves a `UIDescription`, creates a dialog and
  adds it to the gameplay `UserInterface`.
- `analysis/export/Application-steam-x64.exe/decomp.c`: type metadata at `0x1401cff90` names
  `UIDescription` derived from `Description`.
- `Game/Toolbar.rsc`, `Game/Toolbar_statusbar.crs`, and the `Template/UtilityStatusBar_*` compiled
  resources, inventoried in `docs/GAME-RESOURCE-INVENTORY.tsv` and
  `docs/SAVE-EXTERNAL-REFERENCES.tsv`.
- [GAME-LOOP-STEP-07.md](GAME-LOOP-STEP-07.md): StatusBar bindings, layout callback and packaged
  resource route.

## Next trace

Decode the Toolbar item's action field and follow the utility template's controller configuration.
Then continue the new-game loop into the first simulation manager after calendar/environment
updates, recording each manager's input and output before porting its behavior.
