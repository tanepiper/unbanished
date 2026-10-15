# Tooltips

Slice H covers tooltip dialogs and their controllers. An element's description can name a
tooltip dialog. When the pointer rests on the element, the runtime instantiates that dialog,
sets the element's tooltip string as parameter 0 of `textToolTip`, and runs the dialog's
controllers.

Port files:

- `crates/ui/src/input.rs`: `update_tooltip`, `place_tooltip`, `new_tooltip`,
  `set_parameters`.
- `apps/game/src/toolbar.rs`: hotkeys.
- `apps/game/src/create_bar.rs`: `fill_requirements`.
- `apps/game/src/gameplay.rs`: `fill_tooltip`.

## Tooltip dialogs

| Dialog | Controllers | Used by |
|---|---|---|
| `Dialog/ToolTip.rsc` | none; `textToolTip` = `general:Parameter` (`@0`) | most elements, the speed text |
| `Dialog/ToolTipHotKey.rsc` | HotKeyToolTipUI | toolbar `checkToolbar` and `buttonToolbar` |
| `Dialog/ToolTipToolBar.rsc` | HotKeyToolTipUI, BuildRequirementUI | toolbar build entries (`checkToolbar2`) |
| `Dialog/ToolTipTitleDesc.rsc` | TitleDescToolTipUI | mod browser preview only |
| `Dialog/ToolTipUpgrade.rsc` | BuildRequirementUI | Upgrade window (pending) |

Controller ids are the CRC32 of the UTF-16LE class name:

- `HotKeyToolTipUI` `0x5dd83c93`
- `TitleDescToolTipUI` `0x33e61003`
- `BuildRequirementUI` `0xc3fef549`

## HotKeyToolTipUI (`FUN_14006b2e0`)

When the element's key binding (`Element +0xc0`) is set, the controller looks up its
device/input (`KeyMap::GetDeviceForId/GetInputForId`). It then sets the key's name
(`FUN_14009f0f0`) in two places:

- parameter 1 of `textToolTip` (`toolBar:TipAndHotKey` = `"@0 [@1]"`);
- parameter 0 of `textHotKey` (`toolBar:HotKey` = `"[@0]"`).

The name is `keyNames:{Gamepad|Keyboard|Mouse}{code}` (`"%s%i"`), or `Unbound`.

The UI crate implements this from `Node::hotkey`, the resolved name.

## TitleDescToolTipUI (`FUN_140080c00`)

Sets `textTitle` and `textDesc` parameter 0 from the element's user data, a pair of strings
(`Node::title_desc`).

## BuildRequirementUI (`FUN_14005f120`)

This is the same controller as the CreateBar ([BUILD-TOOLS.md](BUILD-TOOLS.md)). It finds the
ToolbarDescription of the element's user data (the toolbar entry's template) and fills
`imageIcon`, `textTitle` and `labelBuild{i}` (`general:CountParameter` = `"x@0"` with the raw
material's small sprite). The port shares `fill_requirements`. `Ui::new_tooltip` hands the
opened dialog to the game, which fills it and calls `place_tooltip` to re-measure it.

## Toolbar hotkeys (`FUN_1400bede0`)

Each level has a pool of action ids: root `Toolbar0..8` (`0x18..0x20`), depth 1 `Tool0..9`
(`0x23..0x2c`), deeper `Camera0..4` (`0x2d..0x31`). Entries are processed in display order:

- An entry that is not `enabled` (`+0x44`) takes its own `value` (`+0x40`), and that id leaves
  the pool.
- The other entries take the first free id.

The default codes come from `FUN_14009e7c0`: `Toolbar0..8` = keyboard 1..9 (F1..F9), and
`Tool0..8` = 0x0e..0x16 ("1".."9"), `Tool9` = 0x0d ("0").

## Verification

- `BANISHED_SEED=1234 banished --boot-capture OUT 15 --press buttonNew@4.5 --press buttonOk@5.0
  --click-at 941,775@12 --move 985,775@14.5` shows "Roads and Bridges [F4]".
- `--move 898,712@14.5` (the first housing entry) shows the build tooltip: house icon,
  "Wooden House [1]", the description, then ×16 logs and ×8 stone.
- `tools/triage/determinism.sh 22 1234` stays identical.

## Pending

- Pressing the hotkeys. The port names the keys but does not bind them yet.
- User key maps (`KeyMap::Load`): the defaults are used.
- Hover text for entities in the 3D world has not been investigated. The game exe's
  `SetToolTip` callers are all UI controllers (toolbar, ProfessionUI, BuildUI,
  `FUN_140085630`).
