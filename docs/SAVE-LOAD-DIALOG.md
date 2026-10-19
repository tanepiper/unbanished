# Save and Load dialog, and where files live

The dialog, autosave and saving work in both builds; only where the files go differs.

The original executable's folders, which the port uses natively so it can replace the executable
(`apps/game/src/platform/paths.rs`):

| What | Original | Port |
|---|---|---|
| Game data | `FileSystem::GetRootDirectory` (`0x1800ae5a0`, the executable's folder) + `GetPlatformName` "Win" + `GetBuildName` "Data": `WinData` | A folder on the command line, else `WinData` beside the executable, else the repository's `game-data/original/WinData` |
| Saves | `FileSystem::GetSaveDirectory` (`0x1800adf20`): `SHGetFolderPathW(CSIDL_PERSONAL)` + `Banished\`, files `%sSave\%s.sav` | `Documents/Banished/Save/NAME.sav` (`%USERPROFILE%` or `$HOME`; a redirected Documents folder is not followed). `BANISHED_USER_DIR` names the `Banished` folder (tests) |
| Settings | The registry, `Software\Shining Rock Software LLC\Banished` | Still the port's settings file (not yet the registry) |

In the browser the saves are kept in IndexedDB (`save_store.rs`, database `banished-wasm`):
- **Stores:** `files` holds each whole `.sav` by name, and `meta` holds its write time and metadata block.
- **Listing:** the metadata is mirrored into memory when the page starts, so the Save and Load
  dialogs list saves synchronously, as the original's listing does.
- **Writing:** a write or delete updates the mirror at once and the database in the background.
- **Loading:** a read is a request the Load dialog polls. Its spinner runs until the bytes arrive and its
  shortest time has passed.
- **Preview and minimap:** the browser cannot wait on the GPU, so they are copied at the save's frame and handed to
  the loop once mapped (`Renderer::begin_capture`, `poll_capture`), a frame or more later.
  Native saves use the same path.

Natively the store is the folder in the table above. The `save PATH` script command still writes a
file directly (native only).

## The list (`Save::SaveLoad::UpdateList`, `0x1800c4710`)

`apps/game/src/ui/save_list.rs`. Every `*.sav` file becomes a `SaveInfo` (0x58 bytes):
- `+0x10` is the name;
- `+0x18` is the preview;
- `+0x30` is the file time;
- `+0x38` is the file name;
- `+0x40` is the index (−1 for the new-save slot);
- `+0x44` is the status: the metadata's extension value, 2 when the save reads and 0 when it does not;
- `+0x48` is the mods.

The Save dialog (argument −2) starts with a new-save slot and leaves out the autosave
(`GetAutoSaveName` "Auto"). The files are sorted newest first (`qsort` with `0x1800c6620`, which
compares `+0x30` descending). Only the metadata payload is read (`Save::read_metadata`). An
unreadable file shows an empty name and a black preview.

When you save into the new-save slot, the save takes the first free `Save%i` (`GenerateSaveName`, `0x1800c3ac0`). Dates
are `DateTime::AsString` (`0x1800c7d40`): the local date and time joined with a space, as the user's short date
(`GetDateFormatW` `DATE_SHORTDATE`) and time without seconds (`GetTimeFormatW`
`TIME_NOSECONDS`). The port writes the en-US forms.

## The dialog (`Dialog/SaveLoad.rsc`, vtable `0x140165d10`)

`apps/game/src/ui/save_dialog.rs`. The module comment has the details:
- **Opening:** the pause menu's Load (id 2) and Save (id 3) open it (`FUN_1400b8d80`, mode 1 or 0), and so does
  the main menu's Load.
- **Building the table** (`FUN_1400bc1b0`): it lays out `max(count, 6) + 2` cells, rounded down to threes, three per row.
- **Refreshing** (`FUN_1400bbd50`): it shows the list page, or the spinner page with "Please wait", "Loading" or "Saving".
- **Each frame** (`FUN_1400bce80`): when an operation finishes, it lists, closes, or loads.
- **Shortest operation times:** listing takes at least 0.75 s (`DAT_140178824`), saving and loading 1.5 s
  (`DAT_140178860`), and quick save 1.0 s. The spinner turns 180° per second (`DAT_140178920`).
- **Buttons** (`FUN_1400bbcb0`): Save `FUN_1400bb4a0` (asks `confirmSave` over a file), Load
  `FUN_1400bb6b0` (asks `confirmLoad` during a game, then `modsMissing` for each mod the save names),
  Delete `FUN_1400bb320` (asks `confirmDelete`).
- **Confirmations** are message boxes (`FUN_14006daa0`: Cancel 0, Ok 1, Yes 2, No 3). Only Yes goes on.
- **A save** waits one frame for its preview (see docs/SAVE-WRITING.md), then is written.
  The dialog closes when its time is up.
- **A load** closes the dialog and switches to a loading screen, then to the loaded game.

## Autosave (`Dialog/AutoSave.rsc`, vtable `0x140161250`)

`apps/game/src/saves/autosave.rs`. It is installed with the other in-game dialogs (`FUN_14009d0d0`).
- **Timer** (`FUN_14005ea80`): timer 0 runs at the Options interval: 1, 5, 10, 15, 20, 30, 45 or 60
  minutes, and is stopped at index 8. It restarts when you change the option.
- **Firing:** `DialogInstance::Update` counts the timer down by frame time. When it reaches zero it
  posts event `0x13` and starts again.
- **Saving** (`FUN_14005e820`): the save runs unless the pause menu is open, the game is a tutorial (flag bit 10), or a save
  is already running. It writes `Auto.sav` and names it with `globalNames:Autosave` ("Auto Save").
  The operation takes at least 1 s.
- **Spinner:** `savingImage` turns at 90° a second, and it and `animatedImage` show while any save runs.

The Save dialog leaves the autosave out; the Load dialog lists it.

The new-save slot shows the save sprite (`SaveLoad +0x60`, the global data's `+0xb8`):
`Dialog\SpriteSheet.rsc` "SaveLoadEmpty", the icon template's own sprite, so the slot keeps it.

### Deviations

- **Confirmations don't block.** The original's `ShowDialog` is a nested loop; the port leaves the action waiting for the answer.
- **Mods.** The port has none, so every mod a save names is "missing". A mod's version is shown as a number.
- **Corrupt label.** "Corrupt" comes from `globalNames`, the game's `+0xd8` table (`FUN_14009e670`).
- **Date format.** Native builds write the en-US formats, not the user's locale (the browser uses its locale).
