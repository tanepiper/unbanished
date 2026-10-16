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
| **Limits window** (toolbar Reports → Limits) | `UtilityLimits` uses `ResourceLimitUI` (`0xf77895ff`) for every limit row, including textile (1.0.5) and the mod flags (1.0.7) | The building windows have this controller, the utility windows do not: the rows open but do nothing |
| **Path tool** (Reports → Path) | `PathTool` (`FUN_1400e3b80` and its vtable `0x140166aa0`): a click picks the entity under the cursor; `FUN_1400e2be0` and `FUN_1400e2c40` (664 lines) show it | Missing: `UtilityPath` has no UIDescription, and the `CreatePath` tool is not handled |
| **Follow** (citizen window's Follow button) | `Dialog/Follow.rsc` (`FUN_140065080`), the camera following the citizen | Missing |
| **Citizen window buttons** | Bind `FUN_140065b00` finds `buttonProfession`, `buttonHome` and `buttonFollow`; events `FUN_140065a50` | Shown, no events |
| **Center button** of building windows | `StandardDialog.rsc:centerPin` `buttonCenter` (`FUN_140067a30`) | Pin works, Center does nothing |
| **Screenshot keys** (actions 33, 34) | `FUN_14009b270`: a frame copy written to a numbered file (`%i.%s`), with or without the UI | Not read |
| **Path Method key** (action 22) | Switches the road tool's path method | Not read |
| **Scheduling** | Each entity bucket is visited by its quota (`[50, 15, 8, 5, 2]`) | **Done.** Citizens are in bucket 1, the map ambience in bucket 2, and the sites' and citizens' scheduled components run on their visits with their elapsed time. Bucket order, cursors and visit times load and save. The systems still run one after another rather than interleaved per entity |
| **Tutorials and scenarios with a save** | The scenario's `+0x88` save is loaded | **Done.** `Save\Tutorial*.sav` load from the data with the scenario's index (`scenario_game`) |
| **Built wells after loading** | They are in the town's well list | Not registered in the port's water (only wells built after the load) |
| **Achievements after loading** | Instances restored | Not read back (a loaded game starts from no progress) |
| **Floors** | Piles and work points on bridges stand on the floor | Only walking uses floors (`floor.rs`) |
| **Start structures** | Stamp map `+8` | They do not (`gameplay.rs:772`) |
| **Scenario goal and rain emitter in a new game's save** | Written | Not written (docs/SAVE-WRITING.md) |
| **Not ported, from DEVIATIONS.md** | — | Building-window refresh for controllers logged once, the injury check, entity event 0 and the picking mesh on demolition, the death and burial path, the path grid's blocked bit, `FUN_1400c4c60`'s `param_13` |

## Presentation differences

These change no game rules, but are visible:
- **Minimap colours** (lighter and greener in the original).
- **Shadows:** no culling of casters off screen.
- **No antialiasing** (MSAA).
- **Cursors:** only the arrow is drawn; the hand, place and move cursors are not.
- **Resource pick lists** have no icons.
- **The new-save slot's sprite** is the template's empty icon.
- **Dates** use en-US formats rather than the user's locale.

## Not needed for the release

- **Developer windows** that the release toolbar does not show: Debug (`DebugUI`, `FUN_140062340`,
  1,196 lines), Noise (`NoiseUI`), Capture (`SpriteCaptureUI`, `CaptureBoxElement`).
- **Deliberate platform choices:** the settings file instead of the registry; achievements in the
  settings instead of Steam; the browser's storage; wgpu back ends; audio having its own Random.

## Mods (next phase)

These are uncited, mod-only functions:
- **Mod browser, details and upload:** `ModBrowseUI`, `ModDialog`, `ModDetailsDialog` and
  `ModUploadDialog` (`FUN_14006dee0`, `FUN_14006f100`, `FUN_14006f790`, `FUN_140070950`,
  `FUN_1400727f0`, `FUN_1400740f0`, `FUN_1400745d0`, `FUN_140074b70`).
- **The reload dialog:** `Dialog/Reload.rsc`, `FUN_140072110`.
- **Package lookup:** `IO::Modification` and `IO::PackageFile` (`FUN_140071600`, `FUN_140073880`).
- **Save checks:** the mod checks on load (`CheckMod`).
- **1.0.7's mod resource flags:** the Status Bar, Limits and Town Hall rows exist, but are hidden
  without mods.
