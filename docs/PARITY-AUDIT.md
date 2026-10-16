# Parity audit against 1.0.7 (build 170910), vanilla

Audit of 2026-10-16. The target is the last release, 1.0.7 (Build 170910). The
[patch notes](https://banishedinfo.com/patchnotes#patch1.0.7) say it adds to 1.0.6 only mod
resource flags and limits, a new memory allocator and minor fixes. Mods are the next phase and
are listed separately at the end.

## How it was checked

1. **Every class the game registers** (309 `BaseTypeInfo` registrations in the Application),
   checked against the port's code and docs:
   - **277 named in the port.**
   - **15 data-free descriptions.** They have no parser, but every shipped file of these types is
     48 bytes: header only, no fields.
   - **5 descriptions with data are read elsewhere.** These are Picking, Select, Highlight,
     FrustumOutline and CaptureBox; the toolbar's and achievement's "well" files are other types.
   - **The remaining controllers and dialogs are covered below.**
2. **Every dialog controller used by the game's data:** the type-id hash was searched for across
   the extracted resources, and every reports-row utility window was opened in the port.
3. **The release toolbar** (reference capture) against the port's.
4. **Functions:** 9,834 in the Application, of which the port's code and docs cite 1,630.
   - Above `0x140130000` the code is almost all the C runtime and libraries.
   - In the game range (`0x140040000`–`0x140130000`) there are 1,172 uncited functions, about
     33,000 decompiled lines. Most are templates, constructors, destructors and container helpers.
   - The large uncited ones were read and are classified below.
5. **The port's own markers:** `docs/DEVIATIONS.md` (44 deviations, 10 not ported, 5 not traced),
   log messages saying something is not implemented, and the key-map actions without a handler.
6. **The 1.0.4–1.0.7 patch notes' features:**
   - Textile limits and graphs, the per-building limits for crop fields, fishing, foresters,
     hunters, orchards and pastures, and the livestock by-product limit are present.
   - Software cursors, the Status Bar and the Town Hall are present.

## Gaps in shipped vanilla features

| Feature | Original | Port |
|---|---|---|
| **Limits window** (toolbar Reports → Limits) | `UtilityLimits` uses `ResourceLimitUI` (`0xf77895ff`) for every limit row, including textile (1.0.5) and the mod flags (1.0.7) | **Done.** The 11 rows show and set the stock manager's limits. RadiusUI (`0x00585b87`, six building windows) binds as the inert controller it is |
| **Path tool** (Reports → Path) | `PathTool` (`FUN_1400e3b80` and its vtable `0x140166aa0`): a click picks the entity under the cursor; `FUN_1400e2be0` and `FUN_1400e2c40` (664 lines) show it | **Done** (`path_tool.rs`): the commutes home to workplace of a citizen, a workplace's workers or a house's families, as cells and steps |
| **Follow** (citizen window's Follow button) | `Dialog/Follow.rsc` (`FUN_140065080`), the camera following the citizen | **Done** (`follow.rs`) |
| **Citizen window buttons** | Bind `FUN_140065b00` finds `buttonProfession`, `buttonHome` and `buttonFollow`; events `FUN_140065a50` | **Done.** The Profession button was always disabled (an unset field); it now reads the workplace |
| **Center button** of building windows | `StandardDialog.rsc:centerPin` `buttonCenter` (`FUN_140067a30`) | **Done** (`FUN_1400671d0`) |
| **Screenshot keys** (actions 33, 34) | `FUN_14009b270`: a frame copy written to a numbered file (`%i.%s`), with or without the UI | **Done** (`screenshot.rs`; the browser downloads it) |
| **Path Method key** (action 22) | Switches the road tool's path method | **Done** (held: no Manhattan heuristic) |
| **Scheduling** | Each entity bucket is visited by its quota (`[50, 15, 8, 5, 2]`) | **Done.** Citizens are in bucket 1, the map ambience in bucket 2, and the sites' and citizens' scheduled components run on their visits with their elapsed time. Bucket order, cursors and visit times load and save. The systems still run one after another rather than interleaved per entity |
| **Tutorials and scenarios with a save** | The scenario's `+0x88` save is loaded | **Done.** `Save\Tutorial*.sav` load from the data with the scenario's index (`scenario_game`) |
| **Built wells after loading** | They are in the town's well list | **Done.** The saved wells, points and blocks are the water's (`restore_water`) |
| **Achievements after loading** | Instances restored | **Done.** Read back and written into the same objects; the game's goal is kept from the save |
| **Floors** | Piles and work points on bridges stand on the floor | **Done.** Piles, drop points, work points and needs use `floor_ground` |
| **Start structures** | Stamp map `+8` | **Done.** They are stamped as they are built (`build_structures`); picking reads the cells |
| **Scenario goal and rain emitter in a new game's save** | Written | **Done** |
| **Placement overlay** (user report) | `FUN_140106850`: green/red cells, the entrance arrows, the tutorial's yellow target; the work-radius ring (`FUN_140055d90`) | **Done** |
| **Not ported, from DEVIATIONS.md** | — | **Resolved or not reachable:** the building-window controllers are all bound (RadiusUI was the last); entity event 0 and the picking mesh after a collapse follow the current mesh; `FUN_1400c4c60`'s `param_13`/`param_14` are covered (`needs.rs`, `Haul::tool`); the path grid's blocked bit has no effect on mode-0 walks and the road search honours the zones. The injury and death switches are the debug flag `Game +0xd0` bit 8 (debug window only). |

## Presentation differences

These change no game rules, but are visible:
- **Minimap colours:** the user constants are now the original's (no snow or autumn); the light it is drawn with is not traced (docs/MAP-WINDOW.md).
- **Shadows:** casters off screen are not culled. This costs time but changes no pixel.
- **Antialiasing:** done. The Options' MSAA counts the adapter supports; the scene and reflection draw multisampled and resolve before the water's copy and the blit (a change restarts the renderer).
- **Cursors:** done. The six cursors (Arrow, Hand, Hourglass, Move, Query, Place) follow the loop, the tool (slot `+0x60`) and the element under the pointer (`+0x3c`, the title bars' Move).
- **Resource pick lists:** done. The crop, tree, livestock and product combos show their items' icons (`ComboElement::AddItem`'s sprite), the chosen one's on the label.
- **The new-save slot's sprite:** matches. The save sprite is "SaveLoadEmpty", the template's own.
- **Dates:** the browser uses the user's locale; native builds still write en-US formats.

## Not needed for the release

- **Developer windows** that the release toolbar does not show: Debug (`DebugUI`, `FUN_140062340`,
  1,196 lines), Noise (`NoiseUI`), Capture (`SpriteCaptureUI`, `CaptureBoxElement`).
- **Deliberate platform choices:** the settings file instead of the registry; achievements in the
  settings instead of Steam; the browser's storage; wgpu back ends; audio having its own Random.

## Mods

See [MODS.md](MODS.md). Done:
- **Packages:** `.pkm` files in the pack directory (format versions 1 to 3), the list's settings
  (`ModEnabled_`, `ModIndex_`), the override rule and the loaded resources each mod adds.
- **Mods dialog:** `FUN_1400727f0`, `FUN_1400723e0`, `FUN_140072560`, `FUN_140071dc0`,
  `FUN_140072090`. Opened from the main menu and the pause menu (id 7).
- **Details:** `FUN_140070950`.
- **Reload:** `Dialog/Reload.rsc`, `FUN_140072110`; over a game the game is saved to memory and
  loaded again.
- **Saves:** the enabled mods are recorded (`SaveMetaData`), and loading asks about each one
  `CheckMod` flags.

Left:
- **Workshop:** the browse tab, uploads and update notices (`FUN_14006dee0`, `FUN_14006f100`,
  `FUN_14006f790`, `FUN_1400740f0`, `FUN_1400745d0`, `FUN_140074b70`, `FUN_140071600`).
- **The browser's own mod storage.**
- **1.0.7's mod resource flags:** the Status Bar, Limits and Town Hall rows exist, but are hidden
  without mods.
