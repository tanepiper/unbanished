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

## Function coverage (2026-10-17)

`tools/triage/coverage.py` lists the original's functions that no file cites, and with
`--undefined` the code Ghidra never made a function of.

**Application:** 9,851 functions; 1,998 cited (plus those cited since).

| Uncited | Functions | Bytes | Finding |
|---|---:|---:|---|
| Exception unwinders (`Unwind@`, `Catch@`) | 5,037 | 72,939 | Compiler cleanup, nothing to port |
| C runtime and libraries (named, or past the game code) | 447 | 60,650 | Nothing to port |
| Under 200 bytes | 1,995 | 167,099 | Getters, constructors, container helpers; the ones with game logic are cited under their callers |
| 200 bytes or more | 354 | 142,649 | Read one by one; see below |

The 354 larger ones:
- **Container and tree templates (68), serialisers' field helpers (47), constructors (18), the C
  runtime (10):** library code the port has its own of.
- **The CRT `qsort` inlined as a template (26 copies).** It is not stable, so equal keys can come
  out in another order than a stable sort's. The port has it as `ai::crt_qsort_by` /
  `crt_qsort_less`, checked against the original (`tools/emu/candidate_sort.py`). Every place the
  original sorts now uses it with the original's comparator: candidates, market, piles, the
  workplace search (`FUN_140030c00`), toolbar order (`FUN_1400220f0`), professions list
  (`FUN_14001f120`), the trade inventory (`FUN_140016ce0`, `FUN_140016fe0`) and the town hall
  inventory (`FUN_140015c10`, `FUN_140015f10`; by quantity it breaks ties by name).
- **Steam only (not this build):** the Workshop browser and upload (`FUN_14006dd70`,
  `FUN_14006ffa0`, `FUN_140073880`, `FUN_1400745d0`, …).
- **Developer windows:** Debug (`FUN_140061520` and its helpers), Noise (`FUN_140076990`,
  `FUN_1400766a0`), Capture (`FUN_14007e870`, `FUN_14007eb10`).
- **Teardown:** the game's destructor (`FUN_14009cc90`) and its parts.
- **The rest** were already ported under the calling function's citation.

**Re-read 2026-10-17 (229 uncited functions of 200 bytes or more, each classified by its body):**
- 103 containers (array growth, tree insert, erase and rebalance, struct copies), 38 serialiser
  field helpers, 14 UI, dialog and Steam Workshop helpers, 5 path-thread command queue entries
  (`FUN_1400abf00`, `FUN_1400abff0`, `FUN_1400ac300`, `FUN_1400ac3d0`, `FUN_1400ac4b0`; the port
  runs these commands directly), 3 string comparators of sorted lists.
- 16 copies of two handler-registration templates (282 and 266 bytes).
- The CRT `qsort` inlined into the market, workplace and AI sorts (`FUN_14002c3d0`,
  `FUN_1400314f0`, `FUN_14002cce0`, `FUN_14002c6e0`), already checked through `crt_qsort`;
  the CRT's `memset`, error path and maths internals.
- The Debug window, the camera's `Transform3` maths, a particle description's field helper,
  river and terrain-stage helpers (checked through their callers by the emulator tests).
- `FUN_14010dbf0` gathers an entity's and its parts' interact and build components for
  construction (the port's `interact_entities`).
- Not reproducible: the entity manager's every-iteration update sets (`FUN_1400c8f80`,
  `FUN_1400c90a0`, entity flags `0x10` and `0x04`) and a commute set (`FUN_140025d80`, an int
  then the address) are trees ordered by memory address. The port uses creation order, which is
  what allocation order normally gives, as for the follow tie-break.

**Code outside Ghidra's functions:** 1,384 targets of read-only data pointers start no known
function (19,774 bytes; the largest 147). Most are type-name and size getters. The event
dispatchers among them hide their handlers from the call graph, and two handlers were found
this way:
- **StatusBarUI's icons** (dispatcher `0x14007f870` → `FUN_14007f5f0`): a click on a status icon
  goes to the next citizen with that problem. Now ported; the icons also take the mouse
  (`SetFlagsOnOff(0x10000, 0)`), which the port had read as clearing it.
- **ResidenceUI's goto buttons** (`0x1400bacf0` → `FUN_1400bab70`) and PastureUI's kind combo
  (`0x1400ba2f0` → `FUN_14007dc80`): already ported, now cited.

**Runtime:** 15,261 functions; the UI controls the game uses are ported from their `Update` and
`Draw` (Slider, Scroll, Combo, Edit, Button, Graph, AnimatedImage, Container, Label). The rest is
the engine the port replaces (renderer, audio, file system, threads). One undefined dispatcher
(`0x1800b9ac0`) runs the save files' background jobs: list, load (`FUN_1800b8e10`, which checks
a CRC over the entry table), delete and write. The port reads and writes saves itself and does
not check that CRC, so a corrupted save loads where the original refuses it.

**Re-read (2026-10-17, later):** all 229 uncited Application functions of 200 bytes or more (every
one is called from cited code), every one read by hand: red-black tree and array
helpers, the type-registration stubs (`FUN_1400031f0` callers), inlined `qsort` copies, the CRT's
`tanf`/`atanf`/error box, Debug and Steam windows, destructors and the finished building's mesh
hand-off (`FUN_14010dbf0`), templated array resizes and `ExternalPtr` array serialisers, the
string sorts already ported as `crt_qsort_less`, dialog constructors and loaders, and the path
finder's thread commands (`FUN_1400abf00`, `FUN_1400abff0`, `FUN_1400ac300`, `FUN_1400ac3d0`,
`FUN_1400ac4b0`; the port solves paths synchronously). None holds a game rule the port lacks. One ordering detail: the
entity manager keeps its post-iteration set (`+0x78`, entities with flag `0x10`,
`FUN_1400c8f80`) in a tree keyed by object address, so `FUN_1400c98a0` visits it in memory
order. That phase only smooths each natural resource's own growth scale (`FUN_140125370`), so its
order changes nothing.

## Engine maths the simulation depends on (2026-10-17)

Replaced engine pieces were checked for whether the simulation's numbers depend on them.
These do, and each is now ported bit for bit:
- **CRT trigonometry** (`crates/game/src/crt.rs`): MSVC's `sinf` (`FUN_140137160`, Runtime
  `0x18012a620`), `cosf`, `acosf` and the Runtime's table-driven `atan2f` (`0x180128f10`, a
  table of `atan(k/256)` rounded down) are not correctly rounded. A correctly rounded `sin` gives
  a different float for about 1 in 1000 inputs, and `acosf` for about 1 in 50. Ported in the
  original's operation order and checked on 800,059 inputs (`tools/emu/crt_math.py`). `powf`,
  `floorf` and `sqrtf` already matched.
- **Normalize, FromYawPitch and Heading** (`math.rs`): `v * (1 / length)` with zero left as is;
  `(−cos p · sin y, cos p · cos y, sin p)`; `atan2f(−x, y) · 57.29578`.
- **Model transforms** (`math::Transform`; `FromYawPitchOffset` `0x18007e540`, `operator*`
  `0x18001eae0`, `Quaternion::Rotate`, `Transform::Inverse` `0x18005e620`): every entity keeps a
  quaternion and an offset (`+0x148`, `+0x158`). Model-space points go through the quaternion,
  not a 2D rotation: use, build and create points, named points, the quarry pit and the
  terraform meshes. Point headings are `Heading(Rotate(FromYawPitch(yaw, 0)))`, not the yaws
  added and wrapped. Checked on 50,000 cases with the inverse (`tools/emu/transform.py`);
  saves write the same quaternion.
- **Floor picking** (`Picking::Scene::AddMesh` / `Intersect`): the ray is taken into the
  floor's model space (the inverse transform), the hit tested against model-space triangles,
  and the hit normal turned back. Every shipped floor mesh is a single root submesh. The cell
  spot checks (`FUN_1400a4aa0`, `FUN_1400a1210`) stand on floors, and their normals, as the
  original's do.
- **The characters' spatial tree** (`character_tree.rs`) for hunting, the tornado and
  following, checked over 3,000 operations (`tools/emu/character_tree.py`).
- **Path sectors** are marked for a rebuild only where a cell turns walkable or unwalkable, in the
  order the cells changed (`Cells::set_type` logs them). Every type write in the original tests
  for this and marks on the spot, and a rebuilt sector hands out its node ids in mark order.
- **Float time sums:** the TimeGoal compares `Time::AsSeconds` (through f64), and the tornado's
  particle slot alpha is stepped as a float sum, which decides the iteration its entity goes.
- **Achievements** are not checked while user pack files (mods) are enabled
  (`AchievementManager::Update` tests `FileSystem +0x38`, `AreUserPackFilesEnabled`).

**Every Application function that calls Runtime vector, quaternion, transform or scalar maths**
(99 of them) is either cited by the port or read and found to need nothing:
- model and geometry transforms set for drawing (`FUN_140003870`, `FUN_140058480`,
  `FUN_1400a9170`, `FUN_1400e1fa0`, `FUN_140052ec0`; the model ones confirm pitch 0);
- the river generator's rectangle helpers (`FUN_140023b30`, `FUN_140024ba0`; the river output
  is checked against the emulator);
- the camera's listener (`FUN_140107c90`), the Debug window (`FUN_140061200`);
- `FUN_1400c7e10` has no callers;
- `FUN_140045af0` is a no-op (a length measured from a point to itself, never below 0).
`Transform2` is used only by the building decals (`FUN_1400b2310`), which are drawn only.
`Dot`, `Length`, `Lerp` and `Cross` give the port's results in the port's order.

Checked and already matching: `System::Time` conversions, `Random`, Perlin noise, the animation
manager, path finding (solved synchronously instead of on threads), and the triangle test in
picking.

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
- **Browser:** mods are kept in the page's storage, chosen with the file picker or dropped on the
  page.
- **Saves:** the enabled mods are recorded (`SaveMetaData`), and loading asks about each one
  `CheckMod` flags.

Left:
- **Workshop (Steam build only):** the port is the build without Steam (the kit's, as GOG's),
  which hides the Browse tab. Not ported: the Steam build's browse tab, uploads and update notices (`FUN_14006dee0`, `FUN_14006f100`,
  `FUN_14006f790`, `FUN_1400740f0`, `FUN_1400745d0`, `FUN_140074b70`, `FUN_140071600`).
- **1.0.7's mod resource flags:** the Status Bar, Limits and Town Hall rows exist, but are hidden
  without mods.
