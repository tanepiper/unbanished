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
`FUN_140134fd0`, which moves workers beyond the new limit through `FUN_140115450`, below);
`0x1454` goes to the next worker.

### Workplace searches

The searches cover the workplaces registered for a citizen's profession (town manager
`+0x100`). A workplace is a candidate when it is enabled, has room (`+0x60` workers below
WorkComponent `+0x30`) and its entrance can be reached (`FUN_14011ada0`).
`Workforce::find_workplace` / `should_move` port them.

- **`FUN_140115450`** chooses a workplace in one of two ways:
  - **Works anywhere** (profession `+0x4c`: laborers, builders): the candidate whose work comes
    first. `FUN_140115940` sorts the candidates by work priority (`FUN_140115800`, Jobs entry
    `+0x24`) and takes the first.
  - **Otherwise** (`FUN_140115490`): the candidate with the most room, the first of equals.
- **Idle adults** (`FUN_140118340`, adults without school and with an empty action stack,
  before the home search):
  - Without a workplace, they take the one `FUN_140115450` finds.
  - With one, they search again when `FUN_140115eb0` says so, and move (to none if nothing is
    found). For a profession that works anywhere: when the workplace cannot be reached, or
    another enabled one with room has work that comes first. For the others: when the
    reachable enabled workplaces' room differs by more than one (balancing).
- **Evictions.** `FUN_1401350d0` (switched off) and `FUN_140134fd0` (fewer places) move the
  first worker to `FUN_140115450`'s choice, or none, until they fit.

Tests: `professions::tests::workplace_searches` and `evicted_workers_move_on`. In
`herdsmen_keep_a_pasture`, herdsmen hired before the pasture can take them find it.

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
| Destroy `FUN_1401231d0` | 3, 6, 7, 4, 5 | see [Demolition](#demolition-cratesgamesrcdestroyrs) | |

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

## Storage sites (StorageLocation, AbsorbResource, StorageDisplay)

- **StorageLocationComponent** (states `FUN_1401328e0`). State 2 enter (`FUN_140132700`)
  appends the entity to the town's storages (`+0x00`); state 3 enter (`FUN_1401326c0`) removes
  it. Both pass the chain on. Built barns and storage yards thus become town storages.
  The port queues the change (`Construction::storage_changes`) and applies it after the
  iteration's update. The starting town's storages are registered when it is created.
- **Storage yards** have no BuildComponent. They are sites through their controllers:
  AbsorbResource, ClearArea and StorageLocation. The initial state is 0 (AbsorbResource's slot
  `+0x80`).
- **AbsorbResourceComponent** (table `FUN_14002f5d0`). Its only state, 0, has an update
  (`FUN_14010ae20`): every pile on the footprint, by rows, goes into the yard's storage when
  it fits (`FUN_140132250` / `FUN_1401324d0`), and the pile entity is removed. Logs left by
  clearing a yard placed over trees end up stored in it.
- **StorageDisplayComponent** (activate `FUN_140057090`, update `FUN_140056d40` →
  `FUN_140056c10`) is presentation:
  - One mesh instance per footprint cell, by rows, on the cell's ground, turned 90 degrees
    when row + column is even.
  - Each stored item in order fills `size × count / capacity × cells` cells with its pile
    model's last mesh. The next cell gets the mesh for the fraction left
    (`meshes × fraction`, capped at the last); the rest stay empty.
  - The port draws them in `StartingTown::model_instances` every frame rather than on the
    component's schedule.

Test: `a_storage_yard_absorbs_its_logs_and_joins_the_town`. A yard over trees is cleared, the
logs are absorbed, and the yard joins the town's storages.

## Production (`ResourceProduceComponent`, `ConsumeProduce` work)

The wood cutter, blacksmith, tailor, tavern, quarry and mine hold a ConsumeProduceComponent
(a ResourceProduceComponent, constructor `FUN_14011e520`) and a `ConsumeProduceDescription`
(serializer `FUN_14011e5b0`; data in [SHIPPED-PRODUCTION-RECIPES.md](SHIPPED-PRODUCTION-RECIPES.md)).
Setup `FUN_14011e3c0`: the selector `+0x50` from the description's `+0x4c`, one weight 1.0 per
recipe (`+0x58`). `FUN_14011e940` returns a nonnegative selector, or for −1 the first recipe whose
cumulative weight passes `RandFloat × total`.

**States** (`FUN_140130700`): state 2 enter (`FUN_140130680`) asks for 0xd, or 0xe while switched
off (`+0x48`), returning false; the 2 → 0xd transition (`FUN_140130460`) makes every "create"
point's cell empty ground. 0xd enter (`FUN_140130330`): switched off → 0xe; otherwise register
the work (slot `+0x90` `FUN_14011e1d0`, `FUN_140134130(…, 1, 0, …)`: mask 1, no kept priority) and
enable the workplace. 0xe enter (`FUN_1401301f0`): remove the work, disable the workplace (its
workers leave). `FUN_1401307e0` switches off and asks for 0xe; `FUN_1401309f0`/`FUN_140130a70`
are the UI's state queries.

**The work** (vtable `0x140167640`, constructor `FUN_1400ee6d0`): one worker slot per "use"
interact point (`+0x58`), one state per "create" point (`+0x68`: 0 free, 1 under way, 2 holds
output), `+0x48` full, `+0x4c` cycles. Profession: the workplace's (slot `+0x50`).

Try (`FUN_1400ecd10`), for a worker of that profession:

1. Select the recipe. The stock limit (manager `+0x170` slot of the recipe's group,
   `count >= limit`; any group while selecting at random) or an exhausted lifetime
   (`FUN_14011e870`: `produced < +0x48` unless −1; the mine and quarry, 2000) → not now
   (exhausted: switch off).
2. A free slot from `Rand(slots)` onwards.
3. Each item in the building's storage that the recipe neither uses nor makes: carry what fits
   (none → drop-off activity) to a storage that takes it (`FUN_140092a60` put, excluding the
   building), CompleteTask after the pickup (`FUN_1400c4c60` flags 0,1,0,0).
4. Each input short in the building: from a storage holding it, empty-handed, as much as the
   worker carries (`FUN_1400c4c60` count −1, partial, CompleteTask after the delivery);
   otherwise the drop-off activity.
5. Create points: a point holding output (2) whose cell is empty again is free. All free →
   not full; none free → full. With a free point, not full and every input present: work.
   Otherwise each create point's pile is offered (`FUN_1400fdaf0`): the worker hauls the output
   away; if none is taken and a point is free, work when the inputs are present.
6. Work (`FUN_1400c4440`): when `+0x4c >= cycles` (description `+0x40`) this cycle produces
   (`+0x4c −= cycles`, component `+0x68` + 1, the create point → 1). The stack, in execution
   order: ShowTool(work type), MoveTo the use point (walk 2, or 3 for types 2–3, 1 above 7),
   ConsumeRawMaterial of the inputs (producing; selected again), PerformWork (anim
   `type + 9` below 11 else 8, `+0x44` seconds, uses the tool), ShowTool(−1), the description's
   message (`+0x38/+0x3c`, producing), CreateRawMaterial (producing), CompleteTask with
   `(producing ? 0x8000) | (point | 0x100) << 8 | slot`.

Complete (`FUN_1400ecc90`): the slot frees; bit 15 marks the point as holding output; bit 16 adds
a cycle. Fail (`FUN_1400ecc50`): the same without the cycle. Refresh (`FUN_1400eca00`): workers
that lost the task free their slot; then points under way that no remaining worker's task names
(it reads `(param >> 4) & 0xf`) become "holds output". Cancel (`FUN_1400ee400`): every worker's
stack is cleared.

CreateRawMaterial (`FUN_1400cc010`): the output's yield (RawMaterialDescription `+0x40`, or
`+0x44` for an educated worker) × 1.0, in piles of at most the worker's capacity / unit
weight, on the create point's cell or, once it holds something, the nearest free ground
(`FUN_1400a1210`: rings 0..=6, no object or occupant, not water or road, type mask 4), each
turned `RandTol(2) × 90°`; then the stock's production counter and the workplace's tracker
(`FUN_14008e3c0`) and AutoPickupComponent. The piles' Pickup works let laborers carry them to
storage. `FUN_14008e460` maps a raw material's flags to the manager slot.

Port: `crates/game/src/produce.rs`; `jobs::WorkHost::resume` continues a try after its pile
offers (the original offers inline). Verified by `placement_tool::construction_tests::
cutter_yard_makes_firewood` (seed 1234, Medium, firewood limit raised): built at step 6978, the
cutter fetches logs, the first firewood pile appears at 8730 and is hauled away at 9432. Not
ported: status icon bits 2/3, the production counter and tracker, AutoPickup, the description's
message (the quarry), a StorageLocationComponent's drop point (the tavern), the production UI
(ConsumeProduceUI recipe choice, work button, limit editing).

## Gatherers (`HerbalistComponent`, its work, `AutoPickupComponent`)

Port: `crates/game/src/herbalist.rs`. The gatherer's shelter and the herbalist share
HerbalistComponent (vtable `0x14015f980`). It uses ResourceProduceComponent's state table
(`FUN_140130700`), and its slot `+0x90` (`FUN_14004bac0`) makes the work (vtable
`0x1401679c0`, no class name): a worker set `+0x48`, its size `+0x50`, and misses in a row
`+0x58`.

| Description | Fields |
|---|---|
| HerbalistDescription (`FUN_14004bc40`) | count, the natural-resource templates (`+0x20`), the stock slot `+0x30` (gatherer 5, food; herbalist 6, herbs) |
| RadiusDescription | `+0x28` the radius (cells) |
| AutoPickupDescription (`FUN_1400454c0`) | `+0x20` on |

Try (`FUN_1400f3880`):

1. **AutoPickup** (`FUN_1400e9280`). Each recorded Pickup work still registered is offered
   with `Jobs::OfferOne(.., 1)` until one is taken (2). Works no longer registered leave the
   list.
2. **Limit.** The stock slot's limit is reached (also status icon 2), or the workers number
   more than the most allowed: 1.
3. **Search.** There are `(int)sqrt(r²π)` draws. Each draw is `RandTol(r)` twice, redrawn
   until inside the disc and on a map row, around the work's cell. The column check in the
   original can never fail. A candidate cell must:
   - not be water (cell types 5–8);
   - be within path rank `2r` of the entrance (`FUN_140008830`);
   - hold a natural resource of a listed template, grown to 1, unclaimed;
   - have a reachable `NearestAllowedNeighbour(.., 0xe07)` toward the citizen.

   A hit starts the removal sequence (`FUN_1400c7380`) with the auto-pickup flag set. The
   citizen gets the job flags, the misses reset, and the worker joins the set.
4. **Otherwise**, with more misses than draws, the worker drops off what it carries (2), or
   gets 0 when it carries nothing. Below that, it waits (`FUN_1400c7960`: PlayAnimation 0 for
   0.25 s, then CompleteTask), joins the set and counts a miss.

Complete (`FUN_1400f3860`) and fail (`FUN_1400f3850`) remove the worker. Refresh
(`FUN_1400f3620`) drops workers that lost the task. Cancel (`FUN_1400f3fb0`) clears each
worker's stack. The profession is the workplace's (`FUN_1400f4020`).

`NaturalResourceComponent::Remove` with the flag (from `RemoveNaturalResource +0x30`) adds the
new pile's Pickup work to the remover's workplace's AutoPickup (`FUN_140045170`). The port
records the workplace on the pile and adds the work when the pile is activated.

Not ported: the production counter `FUN_14008e3c0` and the status icon. A draw whose column
leaves the map reads outside the cell array in the original; the port skips it.

Verification: `placement_tool::construction_tests::gatherers_harvest_wild_food` (seed 1234,
Medium). A gatherer's shelter is built near the start and two gatherers are assigned. They
harvest wild food, and the AutoPickup records the pile, which is picked up.

## Foresters (`ForesterComponent`, its work)

Port: `crates/game/src/forester.rs`.

- **Component.** ForesterComponent (vtable `0x140169890`) uses the ResourceProduce states.
  - ForesterDescription (`FUN_140124f50`, versioned): the tree template `+0x20`, then, from
    version 1, the stock slot `+0x28` (logs, 0).
  - The component has two toggles, `+0x50` plant and `+0x51` cut. They are saved
    (`FUN_140124e30`), queried by the lodge's buttons (`FUN_140124df0`), and switched by
    `FUN_140124d60`. Turning the last one off, or the first one on, switches production
    (slot `+0x70`).
  - Slot `+0x90` (`FUN_140124c50`) turns both toggles on when both are off and the state is
    not 0xe. It then makes the work (vtable `0x1401677c0`, with the gatherer's fields).
- **Try** (`FUN_1400f0390`). It uses the same AutoPickup offers, worker cap, draws, misses
  and fallbacks as the gatherer, with these differences:
  - The limit only sets the icon.
  - A drawn cell that is not water passes on two conditions. First, it lies in the
    entrance's region (`FUN_1400a1950` relabels; either cell type 5–8 passes too). Second, it
    has no occupant and is within path rank `2r` of the entrance.
  - **Plant.** When planting is on and the cell holds no object, the cell must be reachable
    from the citizen, pass `FUN_1400a4aa0(x, y, 4, 0.97, 1000, 0)` and pass the tree's
    neighbour limits (`FUN_140126100`). The citizen then runs `FUN_1400c3bb0`:
    1. ShowTool 3.
    2. Walk (mode 3) to the cell.
    3. PerformWork anim 0xc for 2.1 s, which uses up the tool.
    4. CreateNaturalResource.
    5. ShowTool −1.
    6. CompleteTask.
  - **Cut.** When cutting is on, the cell holds a natural resource, and the stock slot is
    under its limit, the resource's raw material must have neither flag `0x20` (food) nor
    `0x2000` (herbs). It must also be grown to 1, unclaimed, and next to a reachable cell.
    The citizen then runs the removal sequence with the auto-pickup flag.
- **CreateNaturalResource** (vtable `0x1401660c0`, start `FUN_1400cbb20`). The action fails
  unless the cell has no object and the expected occupant. `FUN_1400a1fd0` must also allow
  it (no cell type 0 or 1), and the cell's flags `+0x28` must be 0. Otherwise it clones the
  template and places it in the cell (`FUN_14012ba10`). Growth starts at 0
  (`FUN_140126280`), then the action draws `Rand(variants)` and `Rand(meshes)` and registers
  the tree.

Not ported:
- the cell byte `+0x2a` and the map's set at `+0xe8` in `FUN_1400a1fd0`;
- the toggle buttons (the toggles default on);
- the status icon.

Verification: `placement_tool::construction_tests::foresters_plant_and_cut`. Two foresters
plant a sapling, cut a tree, and the logs are picked up.

## Fishermen (`FishingComponent`, `FishingWork`)

Port: `crates/game/src/fishing.rs`.

- **Description.** FishingDescription (`FUN_140124640`, versioned): the catch's template
  `+0x20`, the water cell types `+0x2c` (0x180: shallow and deep), and from version 1 the
  stock slot `+0x28` (food, 5).
- **Component** (vtable `0x140169660`). Fields `+0x50` registered and `+0x54` water fraction.
  It hooks into the ResourceProduce states through three slots:

  | Slot | Function | Called from | Does |
  |---|---|---|---|
  | `+0x98` | `FUN_140124350` | after state 0xd's enter | adds 1 to map layer 8 within the radius (`FUN_1401243a0` → `FUN_1400a4e60`) and works out the water fraction |
  | `+0xa0` | `FUN_140124320` | state 0xe's enter, transition 0xd → 3 | subtracts the layer |
  | `+0xa8` | `FUN_1401240e0` | state 0xd's scheduled update | recomputes the water fraction |

  The water fraction is the cells of the water types in the disc around the rectangle's
  centre, divided by half the disc's cells on the map.
- **FishingWork** (`FUN_1400eff90`). It holds a worker per use point (`+0x48`), a state per
  create point (`+0x58`: 0 free, 1 under way, 2 holding a catch), and misses (`+0x68`).
- **Try** (`FUN_1400ef3f0`):
  1. With the stock slot at its limit, answer 1 (icon 2).
  2. Find the first free use point; with none, answer 1.
  3. On create points, a held catch whose cell is empty frees the point; take the first
     free one. With none, the catches on the create points are offered (`OfferOne(.., 1)`).
  4. The use point must be reachable.
  5. Up to ten draws within the radius look for a water cell on layer 8. On a hit, run
     `FUN_1400c3fb0`:
     1. ShowTool 5.
     2. Walk (mode 2) to the use point.
     3. PerformWork anim 0xe for 30 s, facing the point's yaw.
     4. CreateRawMaterial on the create point, with the yield scaled by
        `1 / layer 8 there · water fraction` (`+0x4c`).
     5. CompleteTask with `((point | 0x800) << 4) | slot`.
     6. ShowTool −1.
  6. Without a hit, up to 11 waits (`FUN_1400c7960`, parameter = slot); after that, drop
     off what is carried.
- **Other slots.** Complete (`FUN_1400ef380`) and fail (`FUN_1400ef350`) free the use point
  and, with bit 15, mark the create point as holding a catch. Refresh (`FUN_1400ef100`) is
  ConsumeProduce's. Cancel (`FUN_1400efd00`) clears every worker's stack.

Verification: `placement_tool::construction_tests::fishermen_land_fish`. A dock on the
stream near the start counts on layer 8 with a water fraction of 0.58, and two fishermen
land fish.

## Hunters (`HuntComponent`, its work, `HuntAnimal`)

Port: `crates/game/src/hunt.rs`; the notify is in `crates/game/src/agents.rs`.

- **HuntDescription** (`FUN_140127e30`, versioned). The hunting time `+0x20` (2, × 300 s),
  then from version 1 the stock slot `+0x24` (food).
- **Hunting time.** HuntComponent's scheduled update (`FUN_140127c10`) adds
  `dt · workers / most workers` to `+0x50`.
- **Try** (`FUN_1400f44c0`). Three checks come first, in order:
  - AutoPickup offers;
  - a hunter carrying anything drops it off;
  - the stock slot at its limit, or more workers than allowed, answers 1.

  Then one random cell in the radius, kept on a map row, becomes the target. If it is not
  water, is within path rank `2r` of the entrance, holds no object and is reachable, the
  hunter walks there; otherwise they work 5 s where they stand. A `HuntAnimal` is linked into
  that chain (`FUN_1400c1090`).
- **HuntAnimal** (vtable `0x140166200`). It runs until done. Its notify (`FUN_1400cd2b0`)
  waits for the lodge's time to reach `+0x20 · 300` s (`FUN_140127f50`). It then takes the
  nearest wild animal within 8 (`FUN_140097730`), if not already taken, provided it has a
  cell next to it that the hunter can reach.
- **The attack** (`FUN_1400c1370`, pushed without clearing):
  1. Walk next to the animal.
  2. PerformWork anim 0xb for 4 s with the tool.
  3. RemoveAnimal (`FUN_1400cfcc0`).
  4. A CreateRawMaterial per raw material, on the animal's cell.

  When the attack is set up, the animal stops and is flagged (`FUN_14010c730`), its herd
  moves on, and the lodge's time resets.

Verification: `placement_tool::construction_tests::hunters_take_deer`. Two hunters from a
lodge near the start take a deer, and its venison is left.

## Removal tools (`RemovalTool : DragTool`, vtable `0x140168200`)

The toolbar's Harvest Trees, Harvest Stone, Harvest Iron, Clear Resources, Remove Roads, Remove
Buildings, Cancel and Priority buttons start a RemovalTool for a template holding a
`CreateRemovalDescription` (serializer `FUN_14011ef90`): `+0x20` mark natural resources,
`+0x28/+0x30` the resource templates it marks (empty: all), `+0x38` the marker template
(`Template\Clear.rsc`), then four flags: `+0x40` roads, `+0x41` demolish buildings, `+0x42`
cancel, `+0x43` prioritise. Shipped: ClearTree/ClearStone/ClearIron filter
`NaturalResourceTree`/`Rock`/`Ore`; ClearResources marks every resource.

**DragTool input** (`FUN_140101b80`, fields `+0x28` dragging, `+0x29` waiting, `+0x2c` start
cell, `+0x30` end cell, `+0x34` last sounded cell): MouseLeft pressed on the map starts a drag
and opens the CreateBar (`FUN_140104eb0`). While dragging the end cell follows the cursor
(`FUN_140106740`; a new one plays the move sound), slot `+0x70` gives the size and the bar shows
`WxH` and toolbar message 0, centred on the corner cells (`FUN_140105130`, `FUN_140104140`).
With the button up: on the start cell the tool waits for a second click (`+0x29`); off the map
it plays the error sound every frame until the cursor is back; otherwise slot `+0x68` applies,
thunk (error when it returns false), the bar closes. Slot `+0x48` (`FUN_140101b10`) stops a
drag.

**Apply** (`FUN_140108ab0`), every cell of the rectangle in rows; the cell's entity is the
occupant (`+0x00`), else the object (`+0x08`):

1. `FUN_140109d70`: with `+0x20`, the marker template's CreateDragDescription, a
   NaturalResourceComponent entity whose template is in the filter (or no filter), and
   `FUN_1400a4aa0(cell, drag +0x20, 0, 100, 0, drag +0x24, false, none)` (no occupant, `+0x28`
   zero, cell type 2 or 4): a marker entity (`FUN_1400c8bf0`) at the cell, 1 × 1, added with
   `FUN_1400c9d70`.
2. With `+0x40`, a road cell (`FUN_1400a1bd0`, roads; then the road mesh `FUN_1400b4840`).
3. `FUN_140109c40` with `+0x42`: a marker is destroyed (`FUN_1400c8370`); a DestroyComponent
   being demolished (`FUN_140123be0`) returns to its previous state.
4. `FUN_140109b40` with `+0x41`: a DestroyComponent not in states 3..5 starts demolition
   (`FUN_140123470`).
5. With `+0x43`, a registered WorkComponent work (`FUN_140133fa0`) → `Jobs::Prioritize`.

**Draw** (`FUN_140109520`): each cell of the drag (or the hovered cell) through `FUN_140105d90`
with `Terrain/Selection.rsc` (an 8-frame strip, `u = (u + frame) / 8`): frame 3 opaque red
`0xffff0000` where apply would act (steps 1–5), frame 0 `0x8000ff00` elsewhere. The quads are
two triangles per cell over the terrain vertices, split like the terrain by `x + y` parity
(`FUN_1400b65b0`).

**The marker** (`Clear.rsc`: map, zone, createdrag, decal, work (8 most workers), statemachine,
cleararea, destroy, clearresource). Its controllers are ClearArea, Destroy and ClearResource; the
first gives state 0, so ClearArea registers a `ClearArea` work over the marker's cell
(`FUN_14011ce30`, the felling and piles above). Clear → 1 → 2; ClearResource's state 2 enter
(`FUN_14011da30`, table `FUN_140032150`/`FUN_140032270`) destroys the marker. ClearArea's
WorkComponent `+0x30` limit (8) comes from MaxWorkers since the marker has no WorkPlace.

Port: `apps/game/src/removal_tool.rs`, `selection_overlay.rs`; markers are sites with
`Controller::ClearResource`, destroyed through `Sim::destroy_entity` (work removed, occupant
freed). Remove Buildings and cancelling a demolition become `Command::Demolish` and
`Command::StateMessage { message: 0 }`, once per building; Remove Roads `Command::ClearRoad` (see
[ROADS.md](ROADS.md)). Not ported: the marker's decal, the tutorial's fixed rectangle (settings `+0x80`), the bar's requirement
multiplier (`FUN_14005f060`).

## Demolition (`crates/game/src/destroy.rs`)

DestroyComponent (vtable `0x140169500`, 0x68 bytes; serializer `FUN_140123020`) keeps `+0x48`
the damage time left, `+0x50` damaged, `+0x51` its materials come back, `+0x54` deconstruction
work done (component message 0, `FUN_140122fc0`), `+0x58` the work it needs, `+0x60` what goes in
its place (setup `FUN_140122fd0`: DestroyDescription `+0x20`). DestroyDescription (serializer
`FUN_140123340`): `+0x20` the replacement template (the quarry's and mine's `destroyed` hole;
none for the rest), `+0x28` the damaged material set, `+0x2c` the damaged decal, `+0x30` the
lowest construction mesh a collapse reaches. In component order Destroy comes after WorkPlace and
before Residence (`StoneHouse`: statemachine, cleararea, build, workplace, destroy, residence).

`FUN_140123470` (Remove Buildings, `FUN_140109b40`; the 33000 event of a building window
setting `+0x60` first) clears rubble (state message 1, `FUN_1401239a0`) when damaged (6 or 7),
else runs the machine and requests 3:

| State | Enter / update / exit | |
|---|---|---|
| 3 | `FUN_140122c80` | `+0x54` = 0; the work and every status icon go; damaged → 6, else → 4 and icon 5 (BuildDestroy). Returns false: Residence's `FUN_14012edf0` (icons 6 and 7 off, every family moves out with `FUN_14011b8a0(first, none, -1)`, the residence leaves the town's `+0x20`) and WorkPlace's `FUN_140134230` (the workers leave, the workplace leaves its profession's registry) run too |
| 4 | `FUN_1401224a0` / `FUN_140122390` | BecomeSite (`FUN_1401349f0`); `+0x51` with a BuildComponent; a finished building with goods (`FUN_140122f50`) registers a ClearStorage work (mask 1), else → 5. Update: empty and no hauler on the work (`FUN_140133ef0`, slot `+0x58`) → 5, the work goes |
| 5 | `FUN_140122140` / `FUN_1401215f0` | started (`+0x48` progress ≠ 0): `+0x58` = BuildDescription `+0x30` × stages / 2 (at least 1), a BuildStructure with the destroy flag (`FUN_1400eae90(.., 1)`, mask `0x2001`), whose builders message the DestroyComponent instead of the BuildComponent (`FUN_1400c5b60`). Not started: the refund (with `+0x51`) and the entity goes (`FUN_1400c8370`). Update: at `+0x54 ≥ +0x58` the work goes, BecomeBuilding (`FUN_140134b40`), the entity goes; with a `+0x60` and a finished (or damaged) building, the refund (only for a rebuild or another replacement than the description's) and the `+0x60` entity in its place (same map rectangle, rotation, model position and variant, `FUN_1400c9d70`); else the refund |
| 6 | `FUN_1401229a0` / `FUN_140122960` / `FUN_1401228d0` | damaged: out of the town's storages, its goods lost; no time left → collapse (`FUN_140122aa0`: one construction mesh down, not below `+0x30`). Update: the time runs down to a collapse. Exit: a collapse with time left, and one more on `Rand(2) == 0`. Message 2 → 7 |
| 7 | — | rubble. Message 0 (DestroyUI `0x1772`, `FUN_140122770`): BecomeSite, `+0x51`, `+0x60` = its own template → 5 (rebuilt after deconstruction). Message 1 (`0x1773`, `FUN_140122670`): BecomeSite, `+0x51` with a BuildComponent → 5 |

Message 0 in 4 and 5 (`FUN_140121190`, Remove Cancel and DestroyUI `0x1771` = `FUN_1401238d0`,
which also resets `+0x60`) calls it off: icon 5 and the work go; damaged → BecomeBuilding, 7; no
BuildComponent → 2 (run at once); else by the first MultiObject part's mesh: none → 0, the last
→ 2, else 8; BecomeBuilding. `FUN_140123c80` (damage, fire and tornado) does nothing to a site
not started yet, else sets `+0x50`, `+0x48` and requests 3; `FUN_140123ee0` goes straight to 7.

**Refund** (`FUN_140121a00(x, y, w, h)`): unfinished, the stored materials (`(count − reserved)
/ 2` once work began, else all); finished, half of each BuildDescription material (a quarter
when damaged; not times the stages). Piles of at most `100 / size`, turned `RandTol(2) × 90`, on
the rectangle's cells by rows where the cell has no occupant (`+0x00`), type 2 and ground height
`0 ≤ z < 0.2` (`FUN_1400a48b0` twice); the cell cursor carries over between materials. The
entity's deactivation (`FUN_1400c8370` runs every component's slot `+0x38` at once) lifts its
zone and stamp before the update-5 refund, but the state-5 enter's refund comes first, so an
unbuilt site's materials only land on its `.` footprint cells.

Port: `Controller::Destroy`; `Sim::{demolish, cancel_demolition, destroy_message, damage,
to_rubble}`; replacements are `Construction::rebuilds`, placed by the town after the update
(`StartingTown::apply_rebuilds`). The ResidenceComponent is kept on the site while out of the
town's. Not ported: the damaged material, decal and picking (presentation), DestroyUI.

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
  building status icons, ShowTool meshes, DestroyUI.
- Site state machines run every iteration (scheduled quota not modelled); worker sets are
  ordered by citizen index instead of object address.

Next steps:

2. Roads (Remove Roads, the ClearArea cell phase) and demolition (DestroyComponent, Remove
   Buildings, DestroyUI). Removal tools are ported (`removal_tool::tests::
   clear_tree_marks_and_fells`: ClearTree near the start marks 9 trees, one marker cancelled
   with ClearCancel, the other 8 felled by step 1225).
3. Emulator check of the manager (`FUN_1400fdd70`, `FUN_1400fca20`, `FUN_1400fce80`) with fake
   works, like `tools/emu/storage.py`.
4. Drop-off activity 13 and the redirect; families for the profession searches.
