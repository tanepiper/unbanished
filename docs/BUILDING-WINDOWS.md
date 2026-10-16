# Building and citizen windows (entity controllers)

Slices C and D: the `UI::DialogController` subclasses that UIDescription attaches to an entity
host ([SELECTION.md](SELECTION.md)). Port: `apps/game/src/building_ui.rs`, bound in
`apps/game/src/entity_ui.rs`. Evidence: Ghidra pseudocode of the Steam x64 executable and
Runtime DLL; `tools/triage/controller.py NAME` resolves a controller's factory and vtable.

## Controllers

A UIDescription controller row names its class by `CRC32(UTF-16LE name)`. Entity controllers
share one slot layout: `+0x18` close/cleanup, `+0x20` bind (find elements), `+0x38` refresh,
`+0x48` element events `(kind 0x12, element id)`. The port refreshes every open host each
frame after the simulation; refresh reads only.

| Id | Class | Vtable | Bind / refresh / events | Port |
|---|---|---|---|---|
| `0x6186b6a4` | StorageUI | `0x140165d80` | `FUN_1400bdcf0` / `FUN_1400bd920` | done |
| `0x3061173b` | StorageCapacityUI | `0x140163d90` | `FUN_140080ad0` / `FUN_1400809a0` | done |
| `0xbdb788dc` | WorkPlaceUI | `0x140164450` | `FUN_14008ce10` / `FUN_14008c550` / `FUN_14008ccb0` | done |
| `0xdf3a1bb2` | StatusIconUI | `0x140163cd0` | `FUN_140080740` / `FUN_140080570` | done |
| `0x03e455a9` | CitizenUI | `0x140161b30` | `FUN_140065b00` / `FUN_140065440` / `FUN_140065a50` | done (no events) |
| `0x293e60a1` | BuildUI | `0x140165620` | `FUN_1400b7410` / `FUN_1400b6f20` / `FUN_1400b7370` | done |
| `0x2e56a4c8` | DestroyUI | `0x140161c30` | `FUN_140066ab0` / `FUN_140066460` / `FUN_1400669e0` | done |
| `0xf77895ff` | ResourceLimitUI | | `FUN_14007ce40` / `FUN_14007cda0` / `FUN_14007ccb0` | done |
| `0xb998cb9f` | ControlUI | | `FUN_140060310` / `FUN_1400601d0` / `FUN_140060150` | done |
| `0x3998642a` | ProductionUI | | `FUN_14007b270` / `FUN_14007aef0` → `FUN_14007af20` | done |
| `0xc93b6026` | PageToggleUI | | `FUN_14007ad20` / `FUN_14007ac50` / `FUN_14007abe0` | done |
| `0x1aff79f6` | AttendanceUI | | `FUN_14005e6a0` / `FUN_14005e5b0` | done |
| `0xa17245fb` | CemeteryUI | | `FUN_14005f9f0` / `FUN_14005f8b0` | done |
| `0x9c93d6c1` | ConsumeProduceUI | | `FUN_14005fd30` / `FUN_14005fb50` / `FUN_14005fc90` | done |
| `0x00585b87` | RadiusUI | | defaults (no bind, refresh or events) | nothing to do |
| `0x1701e613` | ResidenceUI | | `FUN_1400bad20` / `FUN_1400ba6c0` | done |
| `0x061d5f73` | UpgradeUI | | `FUN_14008c260` / — / `FUN_14008c160` | done |
| `0xff9794df` | ImmigrationUI | | `FUN_14006bc90` / `FUN_14006ba90` | done |

Shipped hosts (all `Dialog\StandardDialog.rsc:entity`, page 1):

- StorageBarn and StorageCart: StorageUI on `Dialog\Storage.rsc:storage`, StorageCapacityUI,
  DestroyUI, StatusIconUI on `Dialog\Building.rsc:icons`; the barn also BuildUI and WorkPlaceUI.
- StorageYard: StorageUI on `storageShorter`, capacity, build, destroy, icons.
- WoodHouse: Residence, Upgrade (config `Template\WoodHouse.rsc:upgradeConfig`), StorageUI on
  `storageShort`, build, destroy, WorkPlaceUI, icons.
- Citizen: CitizenUI on `Dialog\Citizen.rsc:citizen`, StorageUI (carried inventory),
  StatusIconUI on `Citizen.rsc:icons`, and the plain buttons `buttonProfession`, `buttonHome`,
  `buttonFollow` (element ids `0x227`, `0x226`, `0x228`).

## ResourceLimitUI

ResourceLimitUIConfig `+0x20` names a stock slot. The `editLimit` spinner (id `0x3039`,
0..999999) shows the stock manager's limit for it (`Game +0xf0 → +0x170`, slot `+0x8`), not
while the player types in it; a change (event `0x15`) sets the limit (gameplay's
`StockWarnings::limits`, which the production limits read).

## ProductionUI

The `production` (else `production3`, `production4`) element's first two children are tables:
this year's and last year's TrackerComponent lists ([TOWN-INFO.md](TOWN-INFO.md)). Each row
from the second holds `labelStorage` (the raw material's sprite and name) and `numberCount`;
rows past the list hide. Its refresh turns the tracker's year (`FUN_14007af20`); the port's
window shows the turned lists without changing the tracker.

## PageToggleUI

Its first check button (id 700) switches the host's `pages` between the working page (0) and
the inventory (4); bind shows the working page when one of the two shows; refresh enables the
button on those two pages.

## AttendanceUI, CemeteryUI

The first progress bar: attendees out of the places (`+0x30` of AttendanceDescription
`+0x20`), text `@0`; graves with a tombstone (`FUN_140047730`) out of the graves (`+0x50`),
text `@0`, `@1`.

## ConsumeProduceUI

`comboProduct` (id 22000) lists the recipes' labels (ConsumeProduceDescription `+0x20` table,
each recipe's `+0x20` key) and shows the selected one (`FUN_14011e940`; a negative selector
would draw one by the weights from the game's Random, so the port shows the first); choosing
one (event `0x14`) sets ConsumeProduceComponent `+0x50` (`Command::SetRecipe`).
`progressRemaining` is `1 − made (+0x68) / lifetime (+0x48)`, text `%i` percent. Not ported:
the items' sprites (the combo lists text).

## ResidenceUI

Each cell of `residenceTable` (else `residenceLongTable`) has `imageMale`, `imageFemale`,
`textName`, `textAge`, `textProfession` and `buttonGoto` (id `300 + cell`). Refresh walks the
ResidenceComponent's families (`+0x50`) in order: per resident the sex's image, `@0` the name
(`+0x28`) and `@0` the age (`+0x3c × 5`), the profession (ProfessionList `+0x40` child or
`+0x48` student for those, else the citizen's `+0x1f0`), the button on; the cells after the
last resident hide their images, clear their texts and turn the button off. The button shows
and selects the resident (as WorkPlaceUI's worker button).

## UpgradeUI

The element is a button (id 33000) whose first child shows the ToolbarDescription sprite of
UpgradeUIConfig `+0x20`'s template (the wood house's: the stone house). A click makes that
template the DestroyComponent's replacement (`+0x60`) and tears the building down
(`FUN_140123470`): `Command::Upgrade`, the template loaded first.

## ImmigrationUI

`textNone`, `textNomad`, `buttonAllow` (id 39000, `0x9858`) and `buttonDeny` (`0x9859`).
While the town hall's machine is in 0x11 (nomads waiting) the buttons work and `textNomad`
shows with `@0` the nomads (ImmigrationComponent `+0x60`) and `@1` the town's name; else
`textNone`. The buttons send state messages 7 (take them in) and 8 (turn them away).

## ControlUI

The element is a check button (id 50000). ControlUIConfig `+0x20` is the control, `+0x24` a
component's type id. Refresh: the component's slot `+0x68(control)` gives bit 0 enabled and
bit 1 checked; a click calls its slot `+0x70(control)` (`Command::Control`, `control.rs`):

| Component | `+0x68` / `+0x70` | Controls |
|---|---|---|
| ResourceProduce (consume-produce, fishing, hunting, gathering, market) | `FUN_1401309f0` / `FUN_140130a70` | 0: on unless idle (0xe), available in 0xd and 0xe unless switched off (`+0x48`); the machine's message 0 (`FUN_1401302c0` 0xd → 0xe, `FUN_140130180` 0xe → 0xd unless switched off) |
| Forester | `FUN_140124df0` / `FUN_140124d60` | 1 plant, 2 cut; the first on or last off sends ResourceProduce's toggle |
| CropField | `FUN_140120270` / `FUN_140120190` | 0 plant now (state 2), 1 harvest now (10), 2 farming; all need a crop |
| Orchard | `FUN_1400525f0` / `FUN_140052500` | 0 plant, 1 harvest (10, not this year), 2 tending, 3 cut |
| Pasture | `FUN_14012d2a0` / `FUN_14012cf30` | 0 send all away, 1 half, 2 work |
| Trade, Attendance | `FUN_14005caa0`, `FUN_140044870` / `FUN_14005c960`, `FUN_1400447e0` | 0: the workplace's switch (attendance sends everyone away when off) |

## DestroyUI

- **Bind:** `progressRemove` (`+0x30`) and `progressDestruct` (`+0x38`), `textRemove`,
  `textUpgrade`, `textDamage`, `textFixing` (`+0x40..+0x58`), `buttonReclaim` (`+0x60`, id
  `0x1771`), `buttonDestroy` (`+0x68`, `0x1773`), `buttonRebuild` (`+0x70`, `0x1772`); the
  reclaim button hides for a damaged building (`FUN_140123b60`) or DestroyComponent `+0x50`.
- **Refresh**, not damaged: rebuild, destroy and the damage text hide, reclaim shows;
  `progressRemove` is the storage's fill (`FUN_1401323e0 / +0x38`), `progressDestruct` the
  deconstruction `+0x54 / +0x58`, each with `%i` of `fill × 100 + 0.5`; one of them shows (the
  deconstruction once it has started). `textFixing` when `+0x60` is the building's own template,
  else `textUpgrade` for another than the description's replacement (`FUN_140123ad0`), else
  `textRemove`. Damaged or rubble: rebuild and destroy show, enabled unless the state (pending,
  else current) is 6; the damage text shows; everything else hides.
- **Events:** `0x1771` `FUN_1401238d0` (the demolition is called off), `0x1772`
  `FUN_140123940` (state message 0: rebuild), `0x1773` `FUN_1401239a0` (message 1: clear).

## StorageUI

- **Bind:** `storageTable` (else `storageTable2`) is sized for every RawMaterialDescription the
  external factory holds: `ceil(n / columns)` rows, at least the resource's rows, every cell a
  new instance of cell 0's element (`Dialog\Storage.rsc:storageItem`). Each cell's
  `labelStorage` and `numberCount` are cached.
- **Refresh:** the entity's StorageComponent items (`+0x28`, 16-byte entries of material and
  count) with a non-zero count, sorted with the CRT qsort template (`FUN_140021d50`) by
  `FUN_1400bd840`: localized name `<` via `System::String::operator<` → `StrCompare`, a byte
  comparison on signed `char`. Names are unique, so the order is that total order. Each row sets
  the label's sprite (`+0x20` sheet, `+0x30` name), text (`+0x28` table, `+0x30` key) and count;
  unused cells hide their label and count.

## StorageCapacityUI

`fill = FUN_1401323e0(storage) / capacity (+0x38)` (0 without capacity) into the progress bar
(`+0x128`) and its text parameter 0 as `%i` of `(int)(fill * 100 + 0.5)`.

## WorkPlaceUI

`WorkPlaceDescription` (`FUN_1401348f0`) holds two references, profession `+0x20` and `+0x28`;
storage barns and houses have none, a school names `Profession.rsc:teacher`. The constructor
(`FUN_14008d2c0`) zeroes every field, including the worker cursor `+0x60`.

- **Bind:** `workPlaceCount` `+0x30`, `maxCount` `+0x38`, `workerCountUp/Down` `+0x40/+0x48`
  (ids `0x1450/0x1451`), `jobCountUp/Down` `+0x50/+0x58` (`0x1452/0x1453`) and
  `workPlaceButton` (`0x1454`); the job buttons hide when WorkDescription `+0x20` is clear.
- **Refresh:** hidden without a current profession (WorkPlaceComponent `+0x48`, the builder
  while under construction). The job buttons show when WorkDescription `+0x20` is set or for
  the builder with a BuildComponent. `workPlaceCount`: the profession's sprite (`+0x28`,
  `+0x30`), `@0` workers (`+0x60`), `@1` most workers (WorkComponent `+0x30`, computed by
  `FUN_140133a90` while 0). `maxCount` `@0`: adults (`age >= D+0x44`, no school) of that
  profession. Worker `+` is enabled while there are laborers, `−` while that count is above 0.
  Job `+` while most workers `< WorkDescription +0x2c` (for a builder site
  `min(+0x2c, build points)` via `FUN_140128540(1)`), `−` while `> +0x28` (`> 1` for a site).
- **Events** (`0x12`): `0x1450` `FUN_1400925d0` (free workplace) then `FUN_1400918a0` (hire);
  `0x1451` `FUN_140096650` (fire); `0x1452/0x1453` `FUN_14008cae0(±1)` →
  `FUN_140134fd0`; `0x1454` `FUN_14008cba0` advances the cursor (wrapping) over the workers
  and calls `FUN_140108510`: pin the asking EntityUI (`+0x38 +0xd8`), centre the camera on the
  worker, start the EditTool if needed and select it (`FUN_140102880`).

`FUN_140133a90` (MaxWorkers): WorkDescription `+0x24`, or with `+0x34` set
`(int)(w·h / +0x30 · +0x24 + 1.0)` clamped to `1..=+0x24`. Every reader of WorkComponent
`+0x30` recomputes it while it is 0, so setting the most workers to 0 restores the default.
The port: [`WorkPlace::max`](../crates/game/src/professions.rs), the commands in
`professions::Command`, applied before the next iteration.

## StatusIconUI

Bind caches the children of `icons`. Refresh shows icon `i` when bit `i` of the entity's
StatusIconComponent `+0xf0` is set. Only a controller with a config (`+0x10`) whose `+0x20`
byte is clear also requires the host's active page to be 2 or 3; no shipped StatusIconUI has a
config. Citizens' bits are live (`CitizenState::status`); building bits are not ported (0).

## CitizenUI

Bind finds `imageMale/Female`, `textName`, `numberAge`, `textProfession`,
`textEducatedYes/No`, `textClothing`, `textTool`, `textNameIs`, `textAction`, `groupHearts`,
`groupStars` and, in the host, the three buttons. Refresh:

| Element | Value |
|---|---|
| male/female image | citizen `+0x38` |
| educated yes/no | `+0x210 >= D+0x118` |
| name, "name is" | `@0` = `+0x28` |
| age | `(int)(age +0x3c * 5.0)` (`DAT_140178898`) |
| clothing | entry `+0x1e4` of `Dialog\StringTable.rsc:clothing` (Ragged, Fair, Excellent) |
| tool | entry 0 if `+0xac < 1`, else 1 when `+0xa8 == D+0x110`, else 2, of `:toolType` (None, Iron, Steel) |
| profession | child (`ProfessionList +0x40`) or student (`+0x48`) below adult age or at school, else `+0x1f0` |
| hearts / stars | `FUN_14006b830(+0x50, D+0x70)` / `(+0x80, D+0xa4)`, the Status Bar meter |
| profession button | enabled with work `+0x248` or school `+0x208` |
| action | entry CitizenAI `+0x28` of `:citizenActions`; 9 becomes 16 ("picking up resources") when the job's workplace differs from the citizen's work |

`UI::String(table, index)` is entry `index` in the table's serialized order
(`StringTable::GetIndexFromName` searches that order linearly); `Resources::string_at` ports it.
The child/student profession resources are matched by name (`Profession.rsc:child`,
`:student`), inferred from the ProfessionList layout.

## BuildUI

- **Bind** (`FUN_1400b7410`): `storageTable`, `progress` (`+0x40`, maximum 1.0), `buttonPause`
  (id 6000; disabled without a WorkPlaceComponent, else checked while `+0x50` is clear). Row 0's
  `textTotal` (`+0x48`) gets `@0` "0", `@1` ClearAreaComponent `+0x48`. Each further row `j`
  (`labelStorage`, `textTotal`, kept at `+0x30/+0x38`): for a BuildDescription material, the
  material's sprite and name (RawMaterialDescription `+0x20/+0x28/+0x30`), the label's
  `ResourceTip` tooltip and `@1` = count × stages (`+0x4c`); the row after the materials gets
  `@1` = work (`+0x30`) × stages; later rows are blanked.
- **Refresh** (`FUN_1400b6f20`): nothing once built (work × stages ≤ progress `+0x48`). Row 0
  `@0` = `+0x48 − FUN_14011d800` (footprint cells holding another object). Each material row
  `@0` = the site storage's first entry of that material, count − reserved; the work row `@0`
  = progress. The bar and its `@0%` text: (progress + delivered + removed) / (work × stages +
  cleared total + required), `(int)(f·100 + 0.5)`; without a BuildComponent, removed / total.
- **Event** (`FUN_1400b7370`, 6000): `FUN_1401350d0(+0x50 == 0)` toggles the workplace;
  disabling lets every worker go (`Command::SetEnabled`). Not ported: its status icons.

ClearAreaComponent `+0x48` is written when clearing starts (`FUN_14011ce30`, after the
footprint scan): `FUN_14011d800`'s count then.

## EntityUI page

Refresh `FUN_1400676e0` picks the host's `pages` page every frame: with neither Build nor
Destroy component, page 2 while a ClearArea entity is in state 0, else UIDescription `+0x40`.
With one: page 3 in states 3..=7, page 2 while the build is unfinished (`FUN_14010e860`) or in
state 0, else `+0x40` (page 4 kept when active). The state is StateMachine `+0x52`, else
`+0x50`.

## DestroyUI (pending)

DestroyUI binds `progressRemove`, `progressDestruct`, `textRemove`, `textUpgrade`,
`textDamage`, `textFixing`, `buttonReclaim`, `buttonDestroy`, `buttonRebuild`. Its refresh
branches on the StateMachine state (`+0x52`, else `+0x50`; 6 and 7 are the damaged/destroyed
states, `FUN_140123b60`), the storage fill, DestroyComponent progress `+0x54/+0x58` and target
`+0x60`. Events `0x1771..0x1773` call `FUN_1401238d0`, `FUN_140123940`, `FUN_1401239a0`.
It needs the DestroyComponent (demolition).

## Verification

- `--script 'place Template\CutterYard.rsc:resource@7' --script 'select@8'` (seed 1234, 40 s):
  the construction page lists Log 24 of 24, Stone 2 of 8, Build 0 of 45 at 34% (26 / 77); a
  WoodHouse shows the builder `0 of 2` with both spinners in the host title.

- Scripted captures (seed 1234 New Game and zoom, then `--wheel -12@10`):
  `--click-at 450,230@10.5` shows the barn's sorted contents and live capacity; contents fall as
  citizens fetch equipment. `--click-at 486,316@21.8` selects a citizen showing name, gender,
  hearts/stars, age, clothing, tool, laborer, education and "is idling".
- `tools/triage/determinism.sh 22 1234` stays identical; the controllers take no Random and
  write no simulation state.
- Not yet compared against original screenshots at the same view.
