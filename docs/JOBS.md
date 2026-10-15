# Jobs, professions and construction

How a placed building becomes a construction site, how citizens are given work, and how the
player assigns professions. Evidence: Ghidra pseudocode and disassembly of the Steam x64
executable (`tools/triage/decomp.py`, `types.py`); inferences are marked. Ports:
`crates/game/src/jobs.rs` (job manager), `crates/game/src/professions.rs` (professions,
workplaces, assignment), `crates/game/src/construction.rs` (state machine, construction works).

Status: see [Port status](#port-status) at the end.

## Classes involved

| Class | Parent | Notes |
|---|---|---|
| `Work` (`0x845c4242`, base vtable `0x140167e30`, ctor `FUN_1400fc7b0`) | `Object::Internal` | base of every job |
| `GatherRawMaterials` (vtable `0x140167840`, ctor `FUN_1400f1be0`) | `Work` | laborers bring a site its materials |
| `BuildStructure` (vtable `0x1401674c0`, ctor `FUN_1400eae90`) | `Work` | builders build (or take down) |
| `ClearArea` (vtable `0x140167540`, ctor `FUN_1400ebe30`) | `Work` | clearing resources/piles under a footprint |
| `Pickup` (`FUN_1400f90d0`) and the rest (`BuildRoad`, `PlantArea`, `FishingWork`, …) | `Work` | other professions |
| `WorkComponent` (vtable `0x14016b010`) / `WorkDescription` | `Component` | an entity's registered work |
| `WorkPlaceComponent` (vtable `0x14016b160`, ctor `FUN_1401347b0`) / `WorkPlaceDescription` | `ControllerComponent` | workers of a building |
| `BuildComponent` (main vtable `0x140168680`, ctor `FUN_14010e490`) / `BuildDescription` | `ControllerComponent` | construction site |
| `ClearAreaComponent` (vtable `0x140168af0`, ctor `FUN_14011d470`) / `ClearAreaDescription` | `ControllerComponent` | clearing before building |
| `StateMachineComponent` (vtable `0x14016a950`, ctor `FUN_140130f00`) | `Component` | runs the controllers' states |
| `ZoneComponent` (vtable `0x14016b260`) | `Component` | reserves the footprint at placement |
| `Profession` / profession manager (`Profession.rsc:resource`) | `Object::External` | `Game +0xb0` |

## Work base (`FUN_1400fc7b0`, serializer `FUN_1400fc8d0` version 3)

`+0x10` owning entity, `+0x18` the owner's WorkComponent, `+0x20` id, `+0x24` priority, `+0x28`
cell = `(y + h/2)·n + x + w/2` of the owner's map rectangle, `+0x2c` the manager pass in which it
last answered "not now", `+0x30/+0x38/+0x40` a patience clock (below).

Virtual slots: `+0x20` try to assign `(citizen, &manager clock)` → 0 cannot, 1 not now,
2 taken; `+0x28` a second try (base false); `+0x30` a task completed `(citizen, entity, param)`
→ true removes the work; `+0x38` a task failed; `+0x40` cancel: clear every worker's action
stack; `+0x48` refresh (drop workers that no longer hold its task); `+0x50` the profession that
does it; `+0x58`, `+0x60` (profession move, `FUN_1400fd8d0`).

`FUN_1400fc3c0(work, citizen, cell_a, cell_b, &clock)` (Gather and ClearArea before assigning):
`+0x30 += clock − +0x38`; laborers, and citizens without a workplace, pass. Otherwise, with
CitizenDescription `+0xfc` (near), `+0x100` (far), `+0x104` (minutes), `+0x108` (always ok),
`+0x10c` (temperature): `limit = Time(+0x104 × 300)`; if the town has more than twice the work's
`+0x40` citizens … (see the decompilation); a path rank beyond the time-scaled range counts
`+0x40` and fails; a rank within `+0x108`, or the environment temperature (`Game +0x130 → +0x18`)
at least `+0x10c`, passes.

## Job manager (`Game +0x118`, ctor `FUN_1400fe650`)

| Offset | Field |
|---|---|
| `+0x00` | last id (`AddWork` increments first, so ids start at 1; `0x7ffffffe` wraps to 0) |
| `+0x04` | next sequential priority (from 0) |
| `+0x08` | pass (from 1; `+1` per simulation iteration after the stock update, `FUN_14009b850`) |
| `+0x10` | tree priority → id |
| `+0x30` | tree id → work (count `+0x38`; `AssignWork` does nothing when 0) |
| `+0x40 + (c−1)·0x10` | category `c` = 1..14 → ids (mask bits 1..14) |
| `+0x120` | profession → tree of ids (the work's slot `+0x50`) |
| `+0x130` | clock: `+= dt` per iteration |

All trees are keyed by id, so iteration is in id order.

- **AddWork** `FUN_1400fdd70(work, mask, keep, priority)` returns the id. Without `keep` the
  priority is the next sequential one; with it, the given priority moved down (−1) past taken
  values. Bits 1..14 of the mask select categories; bit 0 is unused by the manager.
- **RemoveWork** `FUN_1400fe300(id)`: out of the categories, priorities and profession trees,
  then slot `+0x48`, `+0x40` (cancel), delete.
- `FUN_1400fefa0(id)`: priority = lowest − 1 (the UI's "prioritise").
- `FUN_1400fdc30` / `FUN_1400fdcd0`: slot `+0x30` / `+0x38` by id (CompleteTask start / abort);
  CompleteTask removes the work when `+0x30` returns true.
- `FUN_1400fdaf0(id, citizen, again)`: offer one work (ClearArea offers its piles' works).
- `FUN_1400fe220(cell)`: the first work (id order) at a cell.

**AssignWork** (`FUN_1400fcd00`, AI activity 9 via `FUN_1401107a0`): when the citizen's
profession (`+0x1f0`) is not the laborer, `TryList` over its profession's works (priority
order); if that fails (or for laborers), `AssignEmergency`; if that fails, `TryList` over the
laborers' works. The activity succeeds when the action stack is non-empty afterwards.

**AssignEmergency** (`FUN_1400fca20`, activity 2 and the step above): a citizen with an
emergency (`FUN_14011b0b0`) gets the list `[14]`. Otherwise builders (`Game +0xb0 → +0x38`) get
`[13]` first, then one list per stock slot whose count `<= limit / 10` (`Game +0xf0 → +0x170`,
slot records `count +0, limit +8`):

| Slot | Categories |
|---|---|
| 5 food | 1 |
| 3 firewood | 2, 4, 10 |
| 4 tools | 3, 6, 4, 10, 12 |
| 0 logs | 4, 10 |
| 2 iron | 6, 12 |
| 1 stone | 5, 11 |
| 10 textile | 9 |
| 7 clothes | 7, 9 |
| 6 herbs | 8 |

Then `Rand(n)` on the game stream picks one list (`Rand(0)` draws nothing and the list is
empty), and its categories are tried in order with `TryList(…, by distance, emergency)`.
Inferred category meanings: 10/11/12 clearing logs/stone/iron (ClearArea), 13 construction,
14 fire.

**TryList** (`FUN_1400fce80(citizen, works, profession, by_distance, emergency)`): with a
workplace (`+0x248`, ignored in an emergency) and `profession == citizen's`, the works owned
by the workplace (`work +0x10`); then, when the profession works anywhere (`Profession +0x4c`:
laborer, builder) or in an emergency, all of them. Sorting: `FUN_1400fd4d0` by priority,
`FUN_1400fd160` by `PathRank(citizen cell, work +0x28, store)` (negative → `n + (int)sqrtf`),
both ascending with ties by id. **OfferInOrder** (`FUN_1400fd030`): skip works whose `+0x2c` is
this pass; refresh, try; 2 stops, 1 stamps `+0x2c`.

Success writes on the citizen (every work's assign): `+0x30` = work id, `+0x54 |= 4`,
`+0xb0 = 0`, and `+0x88 |= 0x20000` when the work's profession is the citizen's.

## Professions (`Game +0xb0`)

`Profession.rsc:resource` (serializer `FUN_1400ae300`) references laborer `+0x30`, builder
`+0x38`, child `+0x40`, student `+0x48`; the list (`+0x20/+0x28`) is
`Profession.rsc:professionList` (laborer, builder, farmer, …, priest). `+0x4c` (works anywhere)
is set only for laborer and builder. Adults (`desc +0x44 <= age`) without a school (`+0x208`)
are the citizens every count and search uses.

Town manager registries (`Game +0xf0`): `+0x100` profession → workplace entities
(`FUN_14008ea70` appends once), `+0x20` residences (`FUN_14012f2b0` on completion), `+0xa0`
citizens.

**ProfessionUI** (`FUN_14007c0e0`): spinner event `0x15`, id `300 + i` → `FUN_14007b890(i)`:
for each step up, `FUN_1400925d0` (shuffle the profession's workplaces with `FUN_14008e940` —
`Rand(n)` per position when there are two or more — then the first enabled one with
`workers < WorkComponent +0x30`), then `FUN_1400918a0`-style search for a laborer:

1. `FUN_1400920c0`: a laborer in the family of someone who already has the profession (homes'
   ResidenceComponent families), nearest by home entrance to the workplace;
2. with a workplace, `FUN_140091ef0`: the homed laborer whose home entrance is nearest (path
   rank `FUN_140008830`, ties to the later citizen);
3. `FUN_140091ac0`: shuffle the residences (`FUN_14008eb70`), the family with most laborers;
4. `FUN_140091980`: the first laborer in citizen order.

The chosen citizen gets the profession and the workplace (`FUN_1401173b0`: leave the old
workplace `FUN_140134cb0`, join the new `FUN_140134ef0`). Each step down, `FUN_140096650`: the
first citizen of the profession without a workplace, else the first worker of the workplace with
most workers, else the first of the profession, becomes a laborer without a workplace.
Entrances (`FUN_14008d480`) draw `Rand(points)` when a building has several use points, so
assignment can draw from the game stream.

**WorkPlaceUI** (`FUN_14008ccb0`, event `0x12`): `0x1450` = one more of the workplace's
profession (`FUN_1400925d0` + `FUN_1400918a0`), `0x1451` = one fewer (`FUN_140096650`) — both
town-wide, like the spinner; `0x1452/0x1453` most workers ±1 (`FUN_14008cae0` →
`FUN_140134fd0`, which evicts workers beyond the new limit through `FUN_140115490` /
`FUN_140115940`); `0x1454` goes to the next worker.

## The state machine

`StateMachineComponent` (flags `0xcd`) links the entity's ControllerComponents in component
order (`FUN_140130b70`: each controller's slot `+0x88` returns its state interface at `+0x28`;
the interface's `+0x30` is the previous, `+0x38` the next) and takes the **first** controller's
initial state (slot `+0x80`; ClearArea 0, Build 1). Runner `FUN_1400aeb30(sm +0x48, time,
scheduled, update)`; fields: `+0` current interface, `+8` state, `+9` its index, `+10` pending
state, `+0xb` pending transition index.

Each controller class registers a static table (`FUN_14002fe60`-style helpers into
`DAT_1401dbca0`, `0xf8`-byte records): state id `+0`, enter `+8`, bit-4 update `+0x10`,
scheduled update `+0x18`, exit `+0x20`, transitions `(target, fn)` at `+0x28` (count `+0xa8`).
The interface slots: `+0` find state, `+8` find transition, `+0x18` enter, `+0x20` exit,
`+0x28` transition fn, `+0x30` / `+0x38` updates; a null callback returns false.

- A state is looked up from the **last** controller backwards; the first that has it becomes
  the current interface. Enter/exit/update are called from there backwards until one returns
  true, so later controllers override earlier ones and a false return passes it on.
- A transition request (`FUN_1400aef90(msg)`) looks `msg` up in the current interface's
  transitions; found, the pending state is `msg`. The runner then calls the exit chain, the
  transition's fn, finds the new state from the last controller and calls its enter chain,
  repeating while requests are pending; then, once, the update chain.
- Slot `+0x40` (every iteration, bit 4) runs `(dt, 0, 1)`, slot `+0x48` (scheduled bucket)
  `(elapsed, 1, 1)`, activate `(0, 0, 0)`.

Tables (enter / scheduled update / exit; → transitions):

| Controller | State | Callbacks | → |
|---|---|---|---|
| ClearArea `FUN_14011d4e0` | 0 | `FUN_14011ce30` / `FUN_14011cdc0` / `FUN_14011cd60` | 1, 3 |
| | 0x15 | `LAB_14011ccb0` / `LAB_14011cca0` / `FUN_14011cc40` | 1, 3 |
| | 1 | `FUN_14011ccf0` | 2, 3 |
| | 2 | `FUN_14011ccc0` | 3 |
| WorkPlace `FUN_140134870` | 0 | `FUN_140134310` (returns false) | 1, 3 |
| | 2 | `FUN_1401342c0` (returns false) | 3 |
| | 3 | `FUN_140134230` | 6, 4, 5 |
| Build `FUN_14010e560` | 1 | `FUN_14010d760` / `FUN_14010d6b0` | 8, 3 |
| | 8 | `FUN_14010cfb0` / `FUN_14010cbb0` / `FUN_14010cad0` | 2 (`FUN_14010cab0`), 3 |
| | 2 | — | 3 |
| Residence `FUN_14012f820` | 2 | `FUN_14012f2b0` (returns false) / `FUN_14012ee40` | 3 |
| | 3 | `FUN_14012edf0` | 6, 4, 5 |
| Destroy `FUN_1401231d0` | 3, 6, 7, 4, 5 | (destruction) | |

The wood house's controllers, in component order: ClearArea, WorkPlace, Build, Destroy,
Residence. Its life:

1. **State 0** (ClearArea's initial state). WorkPlace enter (`FUN_1401349f0`): its workers
   leave, its profession `+0x48` becomes the builder, it registers as a builder workplace, its
   most workers is the interact **build** point count (with a BuildComponent), enabled. Then
   ClearArea enter (`FUN_14011ce30`): unless the footprint is already clear (`FUN_14011d910`:
   for the house, every footprint cell's object `+0x08` is empty, `FUN_1400a2f90`), it counts
   the objects (`FUN_14011d800`), builds the clearing category mask (bit 0, plus `0x400` logs,
   `0x800` stone, `0x1000` iron from the objects' raw materials) and registers a `ClearArea`
   work. Clear → transition 1. Its scheduled update transitions to 1 once clear; its exit
   removes the work.
2. **State 1** (Build `FUN_14010d760`): stages `+0x4c` = 1 (or the MultiObject count); the
   ClearArea's `FUN_14011d740` stamps the footprint when the description says so; the site
   joins the town's site list (`Game +0xf0 → +0xd0`). Unless it already holds every material
   (`FUN_14010db00`), it registers a `GatherRawMaterials` work (mask `0x2001`, keep priority) and
   turns its StorageComponent into the material store: empty, capacity = Σ count × weight ×
   stages, accepts the OR of the materials' flags. Otherwise → 8. Its update: all materials
   present → remove the work, → 8.
3. **State 8** (Build `FUN_14010cfb0`): decal alpha 1; every construction model shows mesh 0;
   a `BuildStructure` work is registered (mask `0x2001`, keep priority). (When the game's flag
   `Game +0xd0 & 1` is set — loading/instant — it instead sets the progress to the full amount,
   shows the final mesh and → 2.) Its update (`FUN_14010cbb0`): while progress `+0x48` <
   `work × stages`, each model's mesh = `(int)(progress / total × meshes)`, capped at
   `meshes − 2`; then remove the work, final mesh, → 2. Its exit (`FUN_14010cad0`): entity flag
   4, leave the site list, status icon bit 0 off. The 8 → 2 transition fn (`FUN_14010d4c0`):
   the stored materials leave the stock counters and the storage becomes the building's own
   (description capacity × area when per-cell, accepted flags, empty).
4. **State 2** (complete): Residence registers the house (`FUN_14012f2b0`); WorkPlace
   (`FUN_140134b40`) moves from builder to the description's profession (workers leave, the
   work's profession tree entry moves, most workers from the WorkComponent); ClearArea sets
   entity flag 1.

BuildComponent message 0 (`FUN_14010dbe0`, from a builder's SendComponentMessage) adds 1 to
the progress `+0x48`.

### Footprint at placement

`FUN_140102b20` creates the entity; its `ZoneComponent` activation (`FUN_1401356c0` →
`FUN_1400a2830`) writes the entity as the **occupant** (`+0x00`) of every non-`.` footprint cell
(and sets the path grid's blocked bit), adds 1 to `+0x28` of `.` cells, and registers the
entity by template (`FUN_14008e2f0`). The map **object** (`+0x08`) and cell types are stamped
(`FUN_1400a30f0`) only when clearing is done, which also removes anything left in the
footprint. `MapDescription +0x30` would stamp at activation; houses do not.

## Construction works

### GatherRawMaterials

Fields `+0x48` delivered per requirement (initialised from the site storage's available
amounts), `+0x58` in transit, `+0x68` worker set. Profession: laborer.

TryAssign (`FUN_1400f1310`): requires the site's WorkPlace `+0x50`. For each requirement whose
`delivered + in transit < count × stages`, find a storage holding at least one
(`FUN_140092a60`, mode take); for the first found: amount = remaining, limited by the citizen's
free carrying capacity / weight (none → drop-off activity `FUN_14011b330`, return 2) and the
source's available amount; the site's nearest reachable **build** interact point to the source
point (`FUN_1401280b0`); the patience check (`FUN_1400fc3c0`); then the haul sequence
(`FUN_1400c4c60`), worker added, in transit += amount, return 2. Nothing to do → 1.

`+0x30` (delivered, param `amount << 8 | index`): worker out, transit −, delivered +.
`+0x38`: worker out, transit −. `+0x48`: workers whose stack lost the task leave; if any,
cancel (`+0x40`) and zero the transit amounts. `+0x40`: clear every worker's stack.

Haul sequence `FUN_1400c4c60` (execution order; stack cleared, on empty CitizenAI message 0):
[ShowTool 9 with a barrow] MoveTo source (walk 1, or 5 with a barrow) → PerformWork face
source, clip 6, 0.75 s → PickupStoredRawMaterial (reserved at construction) → ShowTool −2
(carried goods; 10 with a barrow) → PerformWork clip 7 → MoveTo target (walk 4 or 5) →
PerformWork face target, clip 6 → DropOffRawMaterial (redirect allowed) → CompleteTask(work,
param) → ShowTool −1 → PerformWork clip 7. MoveTo speed factors draw `RandFloatTol` when the
MoveTo is built, in push order (target walk first, then source walk).

### BuildStructure

Fields `+0x48` destroy flag, `+0x4c` round-robin cursor, `+0x50` occupied per build point,
`+0x60` worker set. Profession: builder.

TryAssign (`FUN_1400ea490`): requires WorkPlace `+0x50`; workers < build points; when the
site's workplace is full (`workers == WorkComponent +0x30`), only its own workers. The cursor
rotates to a free build point; if the point's cell is connected to the citizen's (`FUN_1400a1950`
regions), it computes the path rank and, for every other point, a connectivity test and node
distance (which only fills the sector distance cache), then the build sequence, worker added,
point occupied, return 2. No connected free point → 1.

Build sequence `FUN_1400c5b60`: `r = Rand(2)` after the stack is cleared; ShowTool (r ? 4 : 7)
→ MoveTo point (walk 2) → PerformWork face site, clip (r ? 13 hammer : 16 saw), 3 s, uses a
tool (no tool: × CitizenDescription `+0x114`) → ShowTool −1 → SendComponentMessage(site,
Build — or Destroy — message 0) → CompleteTask(work, point). `+0x30`/`+0x38` free the point.

WoodHouse: 16 logs + 8 stone, work 30 (30 visits of 3 s); 3 build points.

### Actions

- `CompleteTask` (vtable `0x140165fe0`, ctor `FUN_1400cb270(work id, citizen, entity, param)`):
  start → `FUN_1400fdc30` (and remove the work when done), returns done; abort →
  `FUN_1400fdcd0`. Id `0x7fffffff` means "the citizen's current job `+0x30`".
- `DropOffRawMaterial` (vtable `0x1401661a0`, ctor `FUN_1400cd1a0`, start `FUN_1400cc7d0`):
  move `min(carried, requested)` into the target storage when it fits; otherwise, with the
  redirect flag, search a storage that takes it (`FUN_140092a60` mode put) and push a MoveTo
  there (none → drop a pile `FUN_1400ccd90`, fail). Returns running (done next update).
- `ShowTool` (vtable `0x14015ead0`): attach tool mesh `n` (−2 carried goods, −1/>10 none) to the
  citizen's `tool` node; start returns running.
- `PerformWork` with a tool flag decrements the citizen's tool uses (`+0xac`) on start.

### Material storage search `FUN_140092a60(cell, material, count, mode, exclude)`

Ordinary storages (`+0x00`) then markets (`+0x10`), skipping `exclude`: a StorageLocation's
accepted flags (`desc +0x20`, else storage `+0x3c`) must include the material's; mode 0 needs
`available >= count`, mode 1 `fits`. Rank `PathRank(entrance, cell)` (entrance draws Rand with
several use points); on a strict improvement the target point (`FUN_140132aa0` slot scan, or
`FUN_14004f710` for markets) must exist with its rank below the best, and the best becomes the
entrance rank.

## Clearing, piles and drop-off

### ClearArea work (`FUN_1400ebe30`, vtable `0x140167540`)

Registered by the ClearArea controller's state-0 enter (`FUN_14011ce30`) when the footprint
holds objects. The mask is bit 0 plus `0x400` logs, `0x800` stone or `0x1000` iron for each
resource's raw material. Fields: `+0x48` claimed resources, `+0x58` claimed cells, `+0x68`
template filter, `+0x70..+0x72` resources, cells and piles flags (`ClearAreaDescription
+0x28..+0x2a`). The state-0 exit (`FUN_14011cd60`) removes it.

TryAssign (`FUN_1400eb3b0`):

- Gate: claimed resources + cells must be below the work's most workers, and the site's
  WorkPlace must be enabled.
- Ordering: the footprint's objects (`FUN_1400a50d0`, cells whose object `+0x08` is set) are
  sorted by squared cell distance to the citizen (CRT qsort, `FUN_1400a2c90`).
- **Resources:** for each unclaimed natural resource (passing the filter):
  - it needs the nearest walkable neighbour toward the citizen (`FUN_1400a21c0`, mask
    `0xe07`) and that neighbour must be reachable;
  - the patience check runs next, and a failure returns 0;
  - a resource another sequence holds (flag `0x100000`) is skipped;
  - otherwise the removal sequence is pushed and the resource claimed → 2.
- **Cells:** for roads and fields; not ported.
- **Piles:** each pile's own Pickup work is offered through the manager (`FUN_1400fdaf0`); a
  taken one → 2. The port answers `NESTED` and the manager offers them in order.
- Nothing → 1.

Slots: `+0x30`/`+0x38` free the citizen's claim; `+0x40` clears every claimer's stack. `+0x48`
drops resource claims whose citizen lost the task, but returns at once if a cell claim was lost.

Removal sequence (`FUN_1400c7380`), in execution order:

1. ShowTool(tool `+0x80`).
2. MoveTo next to the resource. Walk is 2 for tools 0, 1 and 4–7, 3 for tools 2–3, else 1.
3. PerformWork with the tool: clip `tool + 9` (8 above 10), for `+0x68`.
4. Only with the animation flag `+0x64`:
   1. AnimateNaturalResource(0, wait) – the fall;
   2. AnimateNaturalResource(1);
   3. PerformWork for `+0x70`.
5. RemoveNaturalResource.
6. ShowTool(−1).
7. CompleteTask.

The resource gets flag `0x100000`, and aborting RemoveNaturalResource clears it.

### RemoveNaturalResource (`FUN_1400cffe0` → `FUN_14012b5c0`)

The resource is queued for removal. A pile is left unless the description names a related
profession (`+0x78`), it is not the laborer, and the citizen does not have it.

- Amount: `yield × min(growth, 1)`, where the yield comes from the pile template's
  RawMaterialDescription: `+0x40`, or `+0x44` for an educated citizen.
- The pile is a clone of the resource's raw-material template at the resource's cell, turned
  `RandTol(2) × 90°` on the game Random. Its MapComponent stamps the cell (cell type 4).
- It takes the priority of a work registered at that cell (`FUN_1400fe220`).

### Piles (`RawMaterialComponent`, vtable `0x14016a5a0`)

- `+0x28` holds the units and `+0x38` the age; the pile is destroyed after 3600 s
  (`FUN_14012e600`).
- Activation (`FUN_14012e6a0`) registers a `Pickup` work with mask `FUN_14012e950` (material
  flag → category) and a kept priority.

### Pickup work (`FUN_1400f83d0`, vtable `0x140167c30`)

State `+0x48`: 0 free, 1 claimed by `+0x50`, 2 picked up.

TryAssign:

1. Without carrying room for the pile, the citizen runs the drop-off activity instead (→ 2).
2. It needs a storage that takes the pile (`FUN_140092890`), a reachable neighbour, the
   patience check and the storage found from the neighbour (`FUN_140092a60`, mode put).
3. The list starts with this pile, plus every unclaimed pile in the 11×11 cells around it
   (rows outer, columns inner) that the found storage takes and that has a reachable
   neighbour. It is sorted by path rank from the citizen (CRT qsort `FUN_14002c9d0`).
4. Every pile that still fits the carried room gets a pickup sequence (`FUN_1400c5fa0`; the
   first clears the stack) and is claimed.

Pickup sequence, in execution order: [ShowTool(−2) when already carrying] → MoveTo neighbour
(walk 4 when carrying, else 1; mask adds the pile cell's type) → PerformWork clip 6, 0.75 s →
CompleteTask → PickupRawMaterial (the whole pile into the carried storage; the pile is
destroyed) → [ShowTool(−2) when not carrying] → PerformWork clip 7 → ShowTool(−1). When the
stack empties, the AI's drop-off activity (13) carries the goods to storage.

Slots: `+0x30` → state 2, `+0x38` → state 0, `+0x40` clears the claimer while claimed, `+0x48`
frees a claimer that lost the task.

### Drop-off activity (`FUN_14011b330`, AI action 13)

For each carried material, it reuses a storage already chosen that takes it, or finds one
(`FUN_140092a60` put, from the citizen's cell). Per storage (`FUN_1400c6540`): MoveTo (walk 4)
→ PerformWork clip 6 → DropOffRawMaterial(all) → ShowTool(−2) → PerformWork clip 7. The
stack ends with a DropOffRawMaterial without a target (its redirect search is not ported), and
the citizen's job resets.

## Port status

Ported (`crates/game`):

- `jobs.rs`: the manager (ids, priorities, category/profession trees, pass/clock), AssignWork,
  AssignEmergency, TryList, OfferInOrder, remove/complete/fail/prioritise/change-profession,
  behind a `WorkHost` trait for the work slots. Unit tests.
- `professions.rs`: profession manager, WorkPlace state, registry, `FUN_14007b890` /
  `FUN_1400918a0` / `FUN_140096650` / `FUN_140134fd0`, UI commands. Unit test.
- `construction.rs`: the state-machine runner (`FUN_1400aeb30`) with the ClearArea, WorkPlace,
  Build and Residence tables; site templates and sites. Unit test of a house's state order.
- `town.rs`: `Sim`, the work host and the state callbacks — WorkPlace becoming a builder site
  and back, ClearArea's clear test and the completion stamp, Build's gather/build/finish
  states with mesh stages, Residence registration; GatherRawMaterials and BuildStructure
  TryAssign/complete/fail/refresh/cancel; the haul and build sequences; the patience check.
- Actions: CompleteTask (job messages, including aborts through `ActionStack::outbox`),
  DropOffRawMaterial (no redirect), ShowTool (state only), PerformWork tool use.
- `stock::find_material_storage` (`FUN_140092a60`), `placement::zone` (`FUN_1400a2830`).
- AI activities 9 and 2 call the manager (`agents.rs`); job and BuildComponent messages are
  delivered in order after each character update.

App (`apps/game`): `PlacementTool::place` registers the entity, reserves the zone and makes a
site (`StartingTown::add_site`); sites and commands are applied before the next iteration
(`apply_commands`); the stock counts only registered storages; the manager steps after the
stock update; ProfessionUI spinners send `SetCount`; `--script "place Template/X.rsc"` and
`--script "profession I N"`.

Verification: `placement_tool::construction_tests::wood_house_gets_built` (seed 1234, Medium):
a wood house on clear ground near the start, two builders, is supplied and built in about
110 simulated seconds; logs −16, stone −8, final mesh, footprint stamped, no reservations left,
no works left. Determinism is unchanged without placements (no work → no draws).

Deviations and gaps (all marked in code):

- ClearArea, RemoveNaturalResource / AnimateNaturalResource, piles, Pickup and the drop-off
  activity are ported. Verified by `placement_tool::construction_tests::
  wood_house_on_resources_gets_cleared_and_built`: the footprint's resource is felled, its
  pile is hauled away and the house is built.
- Deviations: a removed resource leaves on the next animation pass (the original at the end of
  its iteration); piles age every iteration rather than on their bucket visit; a pile takes its
  cell's work priority when it activates in the same delivery; the drop-off's final redirect
  and pile dropping are not ported; the ClearArea empty-cell phase (roads, fields) is not
  ported; the stock's production counter (`FUN_14008e3c0`) is not updated.
- Not ported: families (ResidenceComponent), the drop-off activity `FUN_14011b330`, DropOff
  redirect and pile dropping, emergencies, MultiObject stages, terrain flattening, decals,
  building status icons, ShowTool meshes, the eviction searches `FUN_140115490/940`,
  DestroyUI.
- Site state machines run every iteration (scheduled quota not modelled); worker sets are
  ordered by citizen index instead of object address.

Next steps:

2. Work priority (`Jobs::prioritize`, the ClearPriority tool), CitizenUI profession from
   `CitizenState::profession`.
3. Emulator check of the manager (`FUN_1400fdd70`, `FUN_1400fca20`, `FUN_1400fce80`) with fake
   works, like `tools/emu/storage.py`.
4. Drop-off activity 13 and the redirect; families for the profession searches.
