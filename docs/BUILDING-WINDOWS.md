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
| `0x2e56a4c8` | DestroyUI | `0x140161c30` | `FUN_140066ab0` / `FUN_140066460` / `FUN_1400669e0` | pending |
| `0x1701e613` | ResidenceUI | | | pending |
| `0x061d5f73` | UpgradeUI | | | pending |

Shipped hosts (all `Dialog\StandardDialog.rsc:entity`, page 1):

- StorageBarn and StorageCart: StorageUI on `Dialog\Storage.rsc:storage`, StorageCapacityUI,
  DestroyUI, StatusIconUI on `Dialog\Building.rsc:icons`; the barn also BuildUI and WorkPlaceUI.
- StorageYard: StorageUI on `storageShorter`, capacity, build, destroy, icons.
- WoodHouse: Residence, Upgrade (config `Template\WoodHouse.rsc:upgradeConfig`), StorageUI on
  `storageShort`, build, destroy, WorkPlaceUI, icons.
- Citizen: CitizenUI on `Dialog\Citizen.rsc:citizen`, StorageUI (carried inventory),
  StatusIconUI on `Citizen.rsc:icons`, and the plain buttons `buttonProfession`, `buttonHome`,
  `buttonFollow` (element ids `0x227`, `0x226`, `0x228`).

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
