# Main menu to new-game flow

This note traces the static path from the main-menu button handler into the new-game dialog and the
game-loading loop. Evidence is from Ghidra 12.1.4 headless pseudocode for the Steam x64 Windows
executable plus decoded shipped `.crs` resources. It is not runtime observation or original source.
The executable and Runtime DLL hashes are recorded in [GAMEPLAY.md](GAMEPLAY.md). Ghidra output is
local under ignored `analysis/`.

## Main-menu dispatch

`MainMenuDialog` is registered as a `UI::DialogInstance` subtype at `0x140152cd0`; its initializer
uses vtable `0x140165960`. In `FUN_1400b91a0`, the game looks up named controls and assigns their
element IDs with `UI::Element::SetElementId`. The packaged UI base ID field is zero for these
controls, so the effective action IDs are assigned by game code:

| ID | Named control | Handler result |
|---:|---|---|
| 0 | `buttonNew` | Load `Dialog/NewGame.rsc` and add its dialog instance |
| 1 | `buttonResume` | Close the current dialog |
| 2 | `buttonLoad` | Add `Dialog/SaveLoad.rsc`, passing mode `1` |
| 3 | `buttonSave` | Add `Dialog/SaveLoad.rsc`, passing mode `0` |
| 4 | `buttonOptions` | Add `Dialog/Options.rsc` |
| 5 | `buttonMenu` | Set up a fullscreen-fade/loading transition around `Dialog/MainMenu.rsc` |
| 6 | `buttonQuit` | Call `Application::Quit` |
| 7 | `buttonMod` | Add `Dialog/Mod.rsc` |
| 8 | `buttonAchievement` | Add `Dialog/Achievement.rsc` |
| 9 | `buttonScenario` | Add `Dialog/Scenario.rsc` |
| 10 | `buttonGoal` | Add `Dialog/Goal.rsc` |
| 11 | `buttonMap` | Add `Dialog/MapParams.rsc` |
| 12 | `buttonTutorial` | Add `Dialog/Tutorial.rsc` |

The callback is vtable slot 8 (`0x1400b9020`). It handles these IDs only when event kind `0x12` is
received. The dialog resource itself declares references to the corresponding dialog resources;
the handler selects them and constructs the internal dialog objects. The SaveLoad mode and
asynchronous request paths are traced in [SAVE-FORMAT-AND-RESTORE.md](SAVE-FORMAT-AND-RESTORE.md):
mode 1 follows load and mode 0 follows save.

## New-game dialog construction

The ID 0 path loads `Dialog/NewGame.rsc`, checks its external type against the expected dialog type,
allocates a `0xae8`-byte instance, and adds it to the user interface (`0x1400b8af0`). The instance
uses vtable `0x140162de0`; its element-binding/setup method is `FUN_140075ba0`, and event handler
slot 8 is `FUN_1400755e0`.

The decoded `NewGame_content` lays out controls for town name, map seed, terrain, terrain size,
climate, disasters and starting conditions. `StringTable_mainMenu` supplies those captions as
“Town Name”, “Map Seed”, “Terrain Type”, “Terrain Size”, “Climate”, “Disasters” and “Starting
Conditions”. The dialog also contains a name-generation button and a seed-generation button.

`FUN_14009f280` loads `Game/Climate/ClimateList.rsc`,
`Game/StartCondition/StartConditions.rsc` and `Game/Terrain/TerrainList.rsc` into a shared game-data
object. `FUN_140075ba0` reads those list references and adds their entries to `comboClimate`,
`comboCondition` and `comboTerrain`, respectively. Each added choice keeps the source-list index as
item data. The packaged list contents and registered resource types are:

| Selector | List resource | Item class | Shipped entries in list order |
|---|---|---|---|
| Starting conditions | `Game/StartCondition/StartConditions.rsc` | `MapGenerator` | `Hard`, `Medium`, `Easy` |
| Climate | `Game/Climate/ClimateList.rsc` | `ClimateGenerator` | `Mild`, `Fair`, `Harsh` |
| Terrain | `Game/Terrain/TerrainList.rsc` | `TerrainGenerator` | `Valleys`, `Mountains` |

The same initialization method restores selector indices from shared settings (falling back to
index zero when out of range), generates a town name through `FUN_14009ef90`, and seeds the map-seed
edit with a random integer in the numeric control's allowed range.

That setup method assigns IDs as follows: `buttonNewName` 0, `buttonNewSeed` 1, `buttonCancel` 2,
`buttonOk` 3, and `comboCondition` 4. For event kind `0x12`, the handler maps ID 0 to another
generated name, ID 1 to another random seed, ID 2 to `FUN_140075750`, and ID 3 to closing the
dialog. For event kind `0x14`, ID 4 refreshes the starting-condition description text.

There is a static mismatch that needs a runtime check: `buttonCancel` resolves to the shared button
whose text is “Cancel”, but its assigned ID 2 enters the start path; `buttonOk` resolves to “Ok”,
but ID 3 closes the dialog. The control names, strings, assigned IDs and handler branches all agree
in the decompilation/resource data, so this is recorded as an unresolved shipped behavior rather
than silently swapping the IDs in a port.

## Parameter handoff and loading loop

`FUN_140075750` reads the selected combo indices and writes them into shared configuration fields at
offsets `+0x20` (terrain), `+0x24` (terrain size), `+0x28` (climate), `+0x2c` (starting condition)
and `+0x30` (disaster). It reads the seed from the numeric edit, retrieves the selected
`MapGenerator`, `TerrainGenerator` and `ClimateGenerator` records from their respective lists using
the combos' stored item data, and passes these records together with the town name, seed and
size/disaster values into `FUN_1400b9a10`. The child-loop field layout and first terrain/starting-
condition calls are traced separately in [WORLD-GENERATION.md](WORLD-GENERATION.md); later field
meanings remain unresolved.

`FUN_1400b9a10` constructs a child update loop with `FUN_14009d530`, wraps it in an
`Engine::DialogLoaderLoopT`/`Engine::LoaderLoop` and applies a fullscreen fade. It stores that
transition on the current loop context; the game-loop update method later calls
`Engine::MainLoop::SetNextLoop` when the fade has completed. The child loop's vtable is at
`0x140164880`; slot 10 is `FUN_14001a6a0`, which returns the path string `Game/Game`. This identifies
the game-loop resource name. Its first terrain, starting-condition and climate setup calls are
traced in [WORLD-GENERATION.md](WORLD-GENERATION.md); later generation stages remain open.

## Next evidence to gather

- Verify the Cancel/Ok behavior against a running game or trace the UI callback argument contract.
- Follow the generation helpers and record arrays listed in [WORLD-GENERATION.md](WORLD-GENERATION.md).
- Decode the save container and trace the world-state serializer; see
  [SAVE-FORMAT-AND-RESTORE.md](SAVE-FORMAT-AND-RESTORE.md).
