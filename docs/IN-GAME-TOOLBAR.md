# In-game toolbar

This note describes how the original builds and runs the bottom-right toolbar. The evidence is Ghidra
pseudocode of the Steam x64 build (1.0.7) and the shipped `Game/Toolbar_*.crs` and
`Dialog/Toolbar.rsc` resources. The port is `apps/game/src/toolbar.rs`, and the parsers are
`banished_formats::game::{toolbar_item, toolbar_description}`. All entries parse in
`crates/formats/tests/toolbar.rs`.

## Entries (`Game/Toolbar.rsc:*`)

Each entry is a `Toolbar` object (type 0xc9dd0172). Its fields are read in this order:

| Field | Meaning |
|---|---|
| string table, sprite sheet | external references |
| tooltip, sprite | string key and sprite name |
| kind (`+0x48`) | see below |
| parent | the sub-menu entry it belongs to (none = root bar) |
| template | the building or utility template for kinds 1 and 2 |
| value | the hotkey, used when `enabled` (`+0x44`) is false |
| visible (`+0x45`), enabled (`+0x44`) | hidden entries (capture, debug) are skipped |
| id | camera preset number for kind 5 |
| order | sort key within a level |

For kinds 1 and 2, the icon and tooltip come from the template's `:toolbar` component
(`ToolbarDescription`): sprite sheet `+0x20`, sprite `+0x28`, string table `+0x30` and tooltip
`+0x48`.

`Game/Toolbar_resource.crs` is the `ExternalList` that registers the entries, not an entry itself.

## Building a level (`FUN_1400bede0`)

1. Add `Dialog/Toolbar.rsc`. Its root `main` is a vertical ribbon aligned bottom-right. It holds
   `toolBar` (a radio group with a border), which holds `toolbarContent`, a 30 × 1 table with
   spacing 4 and margins 9.
2. Take every visible entry whose parent is this level, in `order`.
3. Instantiate a template by kind, then put it in table cell `index`:

   | Kind | Template |
   |---|---|
   | 10 | `textSpeed` |
   | 0 | `checkToolbar` |
   | 1 | `checkToolbar2` |
   | other | `buttonToolbar` |

4. Set up each element:
   - name: the entry's object name;
   - tooltip and sprite: as described above;
   - id: the index; user data: the template.
5. Call `RadioElement::FindCheckBoxes`.

Hotkeys come from the entry when it is not `enabled`. Otherwise they are the next free value in
the level's range: root `0x18..0x20`, depth 1 `0x23..0x2c`, depth 2 `0x2d..0x31`.

## Clicks (`FUN_1400be770`)

| Kind | Action |
|---|---|
| 0 | Toggle the sub-menu level for this entry. |
| 1 | Enter placement for the template (`FUN_140108030`). |
| 2 | Open the utility window (`FUN_140106f50`). |
| 3 | Options (`FUN_1401071b0`). |
| 4 | Town info. |
| 5 | Camera preset `id`. |
| 6 | Slower. |
| 7 | Pause (controller `+0x30` = 1). |
| 8 | Play. |
| 9 | Faster. |

Speeds are 1, 2, 5 and 10 simulation iterations per frame. Kind 10 shows the current speed.
Event 0x16 with id 1000 closes the popup.

A sub-menu is centred over its button and sits above the bar:

- right padding = `screenW·uiScale − (button.x0 + button.x1)/2 − popupW/2`, clamped to ≥ 0;
- bottom padding = `toolBar` height (`+0x64`) plus the root's bottom padding (`+0x24`).

## Root bar (1.0.7)

In order: time, reports, housing, transport, storage, services, food, resources, removal, and
options. Options is a kind-3 button with a fixed hotkey. "reports" holds the utility windows
(Status Bar, Event Log, Professions, …) and the nested "camera" sub-menu.

## Port status

The port builds the root bar and one popup level with the original templates, icons and
tooltips. Time controls drive the gameplay calendar, and the speed readout and Pause/Play enabled
states update.

Not yet done:

- placement (kind 1), utility windows (kind 2), options, town info and camera presets, which are
  logged only;
- nested sub-menus (`camera` under `reports`), which replace the popup instead of stacking on it;
- hotkeys and the `uiScale` factor.
