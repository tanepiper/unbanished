# Mods

Banished 1.0.7 loads mods made with the BanishedKit: `.pkm` packages in the pack directory
(`WinData`), in the same container as the game's `.pkg` files. The port reads them, loads the
enabled ones and has the original's Mods dialog. The Steam Workshop parts are not ported.

## Packages

`PackageFile::ReadPackfile` (Runtime `0x1800b6f10`, `crates/formats/src/pkg.rs`). The table of
contents holds these fields, in order:
- count;
- format version;
- game version (`+0x68`, packed: `0x010007ff` is 1.0.7);
- author's version;
- priority (10000 for mods);
- a value at `+0x74` (a mod is skipped when it is not 0);
- `can_override`;
- crc (`+0xa8`);
- the 48×48 icon;
- from version 2, the preview image;
- the name, author and description;
- the entries.

Strings are UTF-16 with byte lengths before version 3 and UTF-8 from version 3. The kit's own
mods are version 1 and made with 1.0.4 (`0x010004fe`).

`Configuration::GetVersionString(v, true)` (Runtime `0x1800c7800`) writes the game version as
`"%i.%i.%i"` of bytes 3, 2 and 1. The low byte adds a stage letter: `0xfb` R, `0xfc` P, `0xfd` A,
`0xfe` B. The long names (" Prototype", " Pre Alpha", " Alpha", " Beta") are used when the flag
is clear.

## The list

`IO::Modification` (`apps/game/src/mods.rs`):

| Function | Address | What it does |
|---|---|---|
| `RefreshList` | `0x1800b0d40` | Reads every `*.pkm`. A mod is enabled by `ModEnabled_<file>` and placed by `ModIndex_<file>`. Its conflicts are the other mods sharing one of its files. |
| `SaveList` | `0x1800b2e50` | Writes both settings for every mod. |
| `Enable`, `Swap`, `DisableAll` | — | Set the dirty byte (`+0`). |
| `SetupFileSystem` | `0x1800b02d0` | Clears the dirty byte. Adds the enabled mods from the last in the list to the first. |
| `FileSystem::LoadPackages` | — | A later package replaces a file only if the file's current owner has `can_override`. |

Because mods load from the last in the list to the first, the first in the list wins.

`Engine::UpdateLoop::LoadResources` (`0x1800e0020`) then loads `Game\GameResources.rsc`, then
each enabled mod's `<file>Resources.rsc:resource`. The game enumerates what loaded by type
(`ExternalFactory::FindNext`): toolbar entries, professions, raw materials and templates.
`crates/formats/src/registry.rs` finds the same set by following resource pointers.

Settings can be frozen. While a loop does not allow mod changes (`UpdateLoop::AllowsModChanges`,
seen by `Engine::Loader::Load`), the list works on a copy and is not saved.

## The Mods dialog

`Dialog/Mod.rsc` (`apps/game/src/mod_ui.rs`). It opens from the main menu or the pause menu's
Mod button (id 7, `FUN_1400b8370`).

Element ids (`FUN_1400727f0`, `FUN_1400723e0`):

| Element | Id |
|---|---|
| Cancel (also Escape) | 100 |
| Ok | 101 |
| A line's checkbox | 1000 + i |
| Up | 2000 + i |
| Down | 3000 + i |
| Top | 4000 + i |
| Details | 5000 + i |

What the buttons do:
- **Lines** (`FUN_140072560`) show the name, author, description and
  `"%i (%s)"` of the author's version and game version. A line's conflict icon shows when the
  mod and another enabled mod share a file.
- **Cancel** reads the list again from the settings.
- **Ok** closes the dialog when nothing changed. Otherwise it asks `Dialog/Reload.rsc:menu` (or
  `:game` over a game), see `FUN_140072110`. On Yes the list is saved and the packages are set
  up again. From a game, the game is first saved to memory and then loaded again.

The details dialog (`Dialog/ModDetails.rsc`, `FUN_140070950`) shows:
- the mod's header;
- its conflicts, with enabled ones highlighted;
- its files. A file is orange when an enabled conflicting mod also has it and this mod wins. It
  is yellow when a mod earlier in the list wins.

## Saves

`SaveLoad::SaveMetaData` (`0x1800c3d00`) records each enabled mod: its name, its file under the
data directory, its crc, its game version and its author's version.

On load, `SaveLoad::CheckMod` (`0x1800c3900`) returns one of these codes. The Load dialog asks
about each saved mod with a non-zero code:

| Code | Meaning | Question |
|---|---|---|
| 0 | Fine | — |
| 1 | Missing | `modsMissing` |
| 2 | Made before 1.0.4 | `modsOutOfDate` |
| 3 | Another author's version | `modsVersion` |
| 4 | Changed crc | `modsChanged` |

In 1.0.7, loading a save does not change which mods are enabled.

## Deviations and gaps

- The details dialog shows a mod's path as `WinData\<file>.pkm`, not the full path.
- Not ported: the Workshop.
  - The Browse tab is empty.
  - Create and Update stay hidden.
  - The download progress and update notice stay hidden, as they do when nothing downloads.
- In the browser the game currently has no way to add `.pkm` files.

Test mods (git-ignored, from banishedinfo.com) live in `game-data/mods/downloads`.
`game-data/modtest/WinData` links them beside the game's packages for captures:

```text
banished --boot-capture out.png 6 game-data/modtest/WinData --setting ModEnabled_apiary=true --press buttonMod@4.5
```
