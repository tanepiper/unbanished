# Citizen AI, actions and movement

How a citizen decides what to do, how that decision becomes a queue of actions, and how the
character walks a path. Evidence: Ghidra 12.1.4 pseudocode and disassembly of the Steam x64
executable and Runtime DLL (local dumps `analysis/citizen-ai-*.c`, `analysis/pathfind.c`,
`analysis/astar.c`, not distributed). Ports: `crates/game/src/pathfind.rs`,
`crates/game/src/character.rs`, `CharacterDescription` in `crates/formats/src/game.rs`.

The citizen template (`Template/Citizen.rsc`) carries three cooperating components:

| Component | Vtable | Role |
|---|---|---|
| `CitizenComponent` | `0x1401689a0` | Needs, age, household (see [CITIZEN-NEEDS-AND-HOUSEHOLDS.md](CITIZEN-NEEDS-AND-HOUSEHOLDS.md)) |
| `CitizenAIComponent` | `0x140168890` | Chooses the next activity |
| `CharacterComponent` | `0x140168790` | Runs the action stack, turns and walks |

## Update phases

Per simulation iteration (`FUN_14009b850`): `Graphics::AnimationManager::Update` advances every
animation, then the entity manager (`FUN_1400c9560`) runs:

1. the bit-`0x04` set: CharacterComponent slot `+0x40` (`FUN_14010e990`) with the iteration delta:
   run the action stack, turn, move by root motion, snap to the ground;
2. the scheduled buckets (quotas 50/15/8/5/2): slot `+0x48` with each entity's elapsed time:
   Citizen needs (`FUN_140118340`), CitizenAI (`FUN_1401148b0`) and Character
   (`FUN_14010e950`, which only forwards to the running chain's slot `+0x30`).

## Character movement (`FUN_14010e990`)

`CharacterDescription` (`FUN_14010f2a0`; shipped citizen values in brackets):

| Offset | Field | Citizen |
|---|---|---|
| `+0x20` | radius (default 0.25) | 0.25 |
| `+0x24` | yaw offset added to the heading | 0 |
| `+0x28` | fraction of the remaining turn kept per update | 0.96 |
| `+0x2c` | animation blend time | 0.5 |
| `+0x30` | speed scale while following another character | 0.75 |
| `+0x34` | speed scale on cell types 1 and 9 | 1.2 |
| `+0x38` | speed scale on cell type 0 | 1.4 |
| `+0x3c`, `+0x40` | hold a fixed height | false, 0 |
| `+0x44` | lowest height | -10 |

Each update (angles in degrees):

- **Turn.** `diff = target − current`, wrapped across ±180. Below 0.1° the heading snaps to the
  target; otherwise `new = (current + diff)(1 − keep) + keep·current`, wrapped to ±180. The
  model yaw is `heading + yaw offset`. Headings come from `Math::Vector3::Heading` =
  `atan2(−x, y)` (0 faces +y).
- **Move.** `distance = |RootAnimationOffset| × speed`, along the direction at `+0x2c`.
  `RootAnimationOffset` is the root translation `sample_pose` already turns into a movement delta
  (`frame root × elapsed × rate`); the citizen walk clip's root moves 0.06 per frame at 15 fps.
- **Ground.** Cell type 11 puts the character at height 0; a fixed-height description uses its
  height; otherwise `max(ground, lowest)` (`FUN_1400a48b0`, which casts against picking geometry
  instead on cells whose byte `+0x2d` is set: bridges, not ported).

`FUN_14010f6c0(anim, blend, scale)` plays a clip and sets `speed = model scale z × scale`.
Replaying a looping clip that is already playing keeps its time (`AnimationState::PlayAnimation`).
Clips blend as `AnimationState` does (`crate::animation::AnimationStack`): each play pushes a
layer and the covered one fades over the group's blend time (`+0x48`, `[new][old]`, else the
default `+0x30`; the description's `+0x2c` handed to `PlayAnimation` is not used). The pose
starts from the oldest layer and mixes each newer one over it by the older layer's share left
(`Quaternion::Lerp`, linear translation). Instanced clips (`+0x2c`; nearly every citizen and
animal clip) follow `AnimationManager`'s shared instances (`crate::animation::shared`): a
group's clip gets `+0x28` clocks (3, 4 or 8) staggered by `duration / count`, each character
joining the one with the earliest clock, so characters walk and work in step groups.

Citizen clips (`Models/Citizen/CitizenAnims`): 0 Idle, 1 Walk, 2 WalkTool, 3 WalkLongTool,
4 WalkInventory, 5 WalkBarrow, 6 BendDown, 7 StandUp, 8 GenericWork, 9 AxeWork, 10 PickAxeWork,
11 ScytheWork, 12 HoeWork, 13 HammerWork, 14 FishingWork, 15 AnvilWork, 16 SawWork.

## Actions and the action stack

Actions are registered serializable types (base `Action`, vtable `0x140165eb0`): `MoveTo`,
`PlayAnimation`, `PerformWork`, `SendComponentMessage`, `PickupRawMaterial`,
`PickupStoredRawMaterial`, `DropOffRawMaterial`, `ConsumeRawMaterial`, `CreateRawMaterial`,
`CompleteTask`, `CompleteRoad`, `CreateNaturalResource`, `RemoveNaturalResource`,
`AnimateNaturalResource`, `HuntAnimal`, `RemoveAnimal`, `ShowTool`.

**ShowTool** (start `FUN_140043070`). `n` above 10 except −2 detaches the tool
(`FUN_14011cbb0`); otherwise `FUN_14011c880` sets the citizen's tool mesh and attaches it to its
mesh's `tool` node (`Geometry::Attach`, a child of `rightHand`):
- `0..10`: CitizenDescription `+0x138` (axe, pick axe, scythe, hoe, hammer, fishing rod,
  hammer, hand saw, bucket, empty and full barrow), at scale 1;
- `−2`: what it carries. That is the item with the most units, shown as its template's
  variant 0 mesh 0 at RawMaterialDescription `+0x50`.

Nothing shows on an entity with flag bit 10. The port draws these as rigid groups placed by the
node's animated world transform (`StartingTown::model_instances`).

Slots: `+0x18` serialize, `+0x20` start, `+0x28` update(dt), `+0x30` notify(dt), `+0x38` stop,
`+0x40` abort. Results: 0 done, 1 running, 2 failed. The base update returns 0.

The Character's stack (`+0x48`, top index `+0x60`) holds chains of actions:

- **Run** (`FUN_1400ca120`): if the top chain has not started, start each action until one does
  not return 1; then update each until one does not return 1. Done (0) stops the started
  actions, pops the chain and continues with the next one in the same run; failed (2) aborts
  everything (`FUN_1400ca8d0`), including the "on empty" chain; running stops the loop. When the
  stack is empty the "on empty" chain (`+0x58`) starts once and is dropped (`FUN_1400ca080`).
- **Push** (`FUN_1400ca9b0`): stop the current top's started actions, then push.
- Builders clear the stack, set "on empty" to `SendComponentMessage(CitizenAI, 0)` and push in
  reverse order of execution. CitizenAI's message slot (`FUN_140114810`) re-runs the Citizen and
  AI updates with zero time, so a finished sequence is followed by an immediate new choice.

| Action | Behaviour |
|---|---|
| `PlayAnimation` (`0x14015ea70`) | start: play the clip; update: `elapsed += dt`, running while `elapsed < duration` |
| `PerformWork` (`0x1401662c0`) | start: play the clip with speed 0, face a yaw within ±180, optionally use up a tool (citizen `+0xac`); update: done once `elapsed ≥ duration`, unless `FUN_14011bb20` (injury check) fails it |
| `SendComponentMessage` (`0x1401664e0`) | start: find the component by type id and call its slot `+0x78`; done, or failed if absent |
| `MoveTo` (`0x140166260`) | below |

### MoveTo (`FUN_1400ce640`, start `FUN_1400cdf80`, update `FUN_1400cda40`)

Fields: target `+0x38`, allowed types `+0x5c` (usually `0xe1f`), walk/stand clips
`+0x468/+0x46c` (1 and 0 for citizens), cost table `+0x470`, speed factor `+0x478`
(`RandFloatTol × 0.1 + 1` from the game stream when constructed with −1), following flag
`+0x474`, path index `+0x60`, count `+0x64`, path cells `+0x68` (256).

- **Start:** done when within 0.05 of the target. Otherwise request a path (`FUN_1400ce0e0`);
  when start and target share a cell the path is that cell (index 0) and the character walks
  straight there.
- **Update:** in the target's cell, walk straight to the target until within 0.05 (done). On a
  path: set the speed for the current cell (following / roads 1, 9 / type 0 / 1.0) times the
  factor; on entering the next waypoint advance and face the one after. At the end of a
  truncated path stand and re-plan. A pending request is collected on the next update (the
  original polls its worker thread's result slot); a failed request fails the action.
- **Stop/abort:** play the stand clip.

## Pathfinding

`FUN_1400a3d90` builds a path manager at map `+0x68`. It runs on a worker thread and executes
queued commands (`FUN_1400ab900`): 0 walk mask, 1 per-type cost, 2/3 cell type/blocked bit,
4 options (diagonal, Manhattan, honour blocked), 5 find path into a result slot. The port solves
synchronously.

**Regions.** `FUN_1400a1950` labels connected walkable cells (types in `0xe1f`) in scan order
from 1 with a FIFO scanline fill (`FUN_1400a1640`). A run is widened by one cell for the
neighbouring rows when the widened cell is strictly inside the row, so diagonal contact
connects regions except at the map edge.

**Requests** (`FUN_1400a2480`): both end types join the mask; unless the mask contains water
types (`0x1e0`) both ends must share a non-zero region. `0x180` in the mask becomes both bits.

**Costs** (`FUN_1400a3a20`, per mover mode; types not listed keep their previous cost):

| Mode | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | diagonal |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 0 (citizens) | 3 | 3 | 4 | 8 | 16 | 16 | 16 | 16 | 16 | 3 | 4 | yes |
| 1 | 16 | 16 | 4 | 4 | 16 | 16 | 16 | 4 | 4 | 4 | 4 | yes |
| 2 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | no |
| 3 | 10 | 10 | 10 | 10 | 10 | 10 | 10 | 5 | 1 | 5 | 10 | yes |

**A\*** (`FUN_14001d9a0`): step cost = type cost × 10 (straight) or 14 (diagonal). Neighbour
order +x, −x, +y, −y, (+x,+y), (+x,−y), (−x,−y), (−x,+y); there is no corner-cutting test.
Heuristic: diagonal search uses octile `10(dx+dy) − 6·min`; otherwise Manhattan ×10 or
Chebyshev ×10. The open set is an AVL tree ordered by `f`, then by the **latest** insertion
(`FUN_14001ddb0`), so ties expand last-in first. Closed nodes reopen when reached more cheaply.
Paths include both ends and are capped at 16,384 cells.

`FUN_1400a21c0`: of the 8 neighbours of a cell whose type is allowed, the nearest (squared cell
distance) to another cell; the first found wins ties.

## Path-rank distance (coarse sector graph)

Storage, job and building searches rank candidates with `FUN_1400a4ce0(map, a, b, store)`, not
with full A*. Port: `crates/game/src/sectors.rs`.

- **Sectors** (`FUN_1400adb40`): `n/8 × n/8` sectors of 8 × 8 cells. Each sector's walkable
  cells are split into areas with an 8-connected FIFO scanline fill, rows outer
  (`FUN_1400ad3f0`/`FUN_1400acfa0`). Each area is a node at the integer mean of its cell
  coordinates (`FUN_1400ace20`); the cells next to the area just outside the sector become its
  border list (`FUN_1400acc20`), and every cell records its node at `+0x18` (`FUN_1400acb40`).
  Freed node ids are reused last-in first-out (`FUN_1400ad6d0`/`FUN_1400ad820`).
- **Links** (`FUN_1400ac950`): a node links to the distinct nodes of its border cells.
- **Changes** (`FUN_1400ae160`): a changed cell marks its sector for rebuilding and the 3 × 3
  sectors around it for relinking; rebuilding is deferred to the next search
  (`FUN_1400adf60`: all node rebuilds, then all relinks, in marking order). The initial build
  marks every sector in index order.
- **Distance:** `-1` unless `FUN_1400a55c0` says the cells are connected; each cell's node is its
  `+0x18`, or the first node found on rings 1–3 around it (`FUN_1400a2570`); the same node gives
  1; otherwise an A* over nodes (`FUN_14001e630`, same open-set order as the cell A*) with costs
  and heuristic `(int) sqrtf(dx² + dy²)` between node positions, summed per hop
  (`FUN_1400ae070`; 0 when no path). Callers treat −1 as `n + |a − b|`.
- **Cache:** results stored with `store` go into a 131,072-entry FIFO ring keyed by node pair
  (`FUN_1400ab000`); each simulation iteration, before `AnimationManager::Update`, the cache
  clock advances and entries older than 120 s are dropped (`FUN_1400ab1e0`). Cached values are
  not invalidated by graph rebuilds.

`tools/emu/sectors.py` → `crates/game/tests/sectors_emu.rs`: on 12 random 64 × 64 maps every
cell's node id (before and after 40 random changes) and 3,600 distances match the original.

## Choosing an activity (CitizenAIComponent)

State: 16 cooldowns (`+0x40..+0xb8`, one per action), current action `+0x28`, queued action
`+0x2c`, urgent-hunger/cold flags `+0x30/+0x31`, time since last loiter `+0xc0`, time carrying
inventory `+0xc8` (reset while empty).

**Update** (`FUN_1401148b0`): count cooldowns down; when hunger exceeds the urgent threshold
(`D+0xcc`) and no urgent action is running or cooling down, eat from home if it has food, then
choose with urgent candidates only; likewise for cold (`D+0xec`); re-choose when an emergency
(`FUN_14011b0b0`, map cell byte `+0x22` at home, work or here) appears; when the character is
idle choose normally.

**Choose** (`FUN_140113060`): run a queued action if it succeeds; otherwise build candidates,
then try each in order until one leaves the character with queued actions; record it as current.
An action only runs when its cooldown has expired (`FUN_1401146c0`).

**Candidates** (`FUN_140113170`; `D` = CitizenDescription, priorities in brackets):

1. sick with a doctor assigned: only 5 [6];
2. hunger > `D+0xcc`: 14 [7]; cold > `D+0xec`: 15 [7]; hunger > `D+0xc8`: 0 [6];
   cold > `D+0xe8`: 1 [6];
3. not sick: emergency → 2 [7]; no clothing (`+0x1e4 == 0`) → 3 [3]; health < `D+0x74` and not
   flagged → 4 [3]; sick → 5 [6];
4. urgent-only selection stops here (no shuffle);
5. 12 [2] when unhappy for long enough (happiness < `D+0xb8` for 75 s, < `D+0xb4` for 300 s, or
   1800 s since the last loiter), else 12 [0];
6. children (age < `D+0x44`) or pupils: without school, 11 [0] (if healthy), 6 [4] without home
   food, 7 [4] without home firewood; at school, 10 [2] (if healthy);
7. adults: with a home, 6 [6 if hungry else 4] unless the spouse is already fetching food, and
   7 [6 if cold else 4] likewise for firewood; then (if healthy) 8 [5] without a tool, 9 [2], and
   13 [3 after 300 s carrying, else 1] when carrying inventory.

The list is shuffled (`Rand(count)` per position, swapping) on the game stream and sorted by
priority, highest first, with the CRT `qsort` (`FUN_140030770`: median of three, short sort
below 9 elements), so equal priorities keep a shuffled order.

| Action | Function | Meaning (inferred from data used) |
|---|---|---|
| 0 / 14 | `FUN_140112250` | eat (home food, then storage) |
| 1 / 15 | `FUN_140112790` | get warm (home, then warm building) |
| 2 | `FUN_140110810` → job manager `FUN_1400fca20` | emergency work |
| 3 | `FUN_140111830` | fetch clothing (storage category 4), 75 s cooldown |
| 4 | `FUN_140111d60` | fetch herbs |
| 5 | `FUN_140112c60` | visit the doctor |
| 6 / 7 | `FUN_140111160` / `FUN_1401108b0` | stock home with food / firewood |
| 8 | `FUN_140111a50` | fetch a tool (storage category 2), 75 s cooldown |
| 9 | `FUN_1401107a0` → job manager `FUN_1400fcd00` | work |
| 10 | `FUN_140112a30` | attend school |
| 11 | `FUN_14010f780` | children: walk near a parent |
| 12 | `FUN_14010fa30` | loiter near a building, then idle 10 ± 5 s |
| 13 | `FUN_14011b330` | drop off carried inventory |

New citizens start without a tool (`+0xac = 0`) or clothing (`+0x1e4 = 0`), so an adult's first
candidates are 8 [5], 3 [3], 9 [2] and 12 [0].

## Verification

Emulating the original functions (`tools/emu/`, compared by tests in `crates/game/tests/`):

- `regions.py` → `regions_emu.rs`: region labels of 20 random 40×40 maps are identical;
- `astar.py` → `astar_emu.rs`: 600 searches on 30 random 32×32 grids with the citizen costs
  return identical paths (547 found, 53 unreachable), reusing one solver as the game does;
- `candidate_sort.py` → `candidate_sort.rs`: 400 random candidate lists sort identically.

## Status of the port

Ported: regions, path manager costs and A* (verified), request validation, character movement,
the action stack, MoveTo, PlayAnimation, PerformWork, SendComponentMessage, the AI's candidate
list, shuffle and sort (verified), choice and timers (`crates/game/src/ai.rs`).

In the game (`crates/game/src/agents.rs`, driven from `StartingTown::update_citizens`) each
citizen has a Character, CitizenAI, equipment state and carried StorageComponent ledger.
Activity 12 (loiter, `crates/game/src/activities.rs`), activities 3/8 (clothing/tool) and
activities 2/9 (emergency work and work, through the job manager, `crates/game/src/jobs.rs`;
see [JOBS.md](JOBS.md)) are ported. The stock locations retain their placed-entity ids, packaged accepted masks and
world-space interaction points. New citizens fetch equipment and then loiter.

## Storage, loose piles and inventories

This section adds static x64 Steam 1.0.7 evidence from the functions below, their disassembly,
and original-function emulation. It does not establish full original-game visual or scheduler
parity. Related evidence: [RESOURCE-PRODUCTION-FLOW.md](RESOURCE-PRODUCTION-FLOW.md) and
[STARTING-INVENTORY-DISTRIBUTION.md](STARTING-INVENTORY-DISTRIBUTION.md).

Stored goods are **not separate entities**. A StorageComponent on a barn, cart, market or citizen
owns an ordered ledger (`+0x28` array, `+0x30` length, `+0x34` capacity). Each 16-byte entry holds
a raw-material **template** reference at `+0`, signed quantity at `+8`, and signed reservation
at `+0xc`. Its component `+0x38` is weighted capacity and `+0x3c` is the accepted category mask.
`RawMaterialDescription +0x48` is unit weight; `+0x5c` is the category flags.

| Routine | Operation |
|---|---|
| `FUN_1401323e0` | Sum quantity × unit weight, including reserved goods. |
| `FUN_140132250` | Require intersecting accepted flags and sufficient weighted capacity. |
| `FUN_1401324d0` | Merge into the first matching template entry; append with zero reservation only when absent. The previous Rust implementation incorrectly appended every addition. |
| `FUN_140132620` | Subtract from the first matching entry; erase only when exactly zero, shifting subsequent entries without reordering. No negative clamp. |
| `FUN_14011ba10` / `FUN_14011bc90` | Citizen remaining weighted capacity / available quantity (stored minus reserved). |

Loose raw-material piles **are entities**, with a different component. RawMaterialComponent
(`FUN_14012e8d0`, vtable `0x14016a5a0`) holds its pile quantity at `+0x28`, an internal reference
at `+0x30`, and a time at `+0x38` (`FUN_14012e800`, version 2). Activation `FUN_14012e790`
initializes that quantity from its description `+0x40`. `CreateRawMaterial`'s start
`FUN_1400cc010` creates a pile entity, positions it and writes the produced quantity to that
component. The exact role of the pile's `+0x30` reference is still open. This fetch slice uses
stored ledgers; loose-pile pickup/production remains separate work.

Starting goods retain the existing `FUN_140096b30` round-robin distribution, but now add to
these same live ledgers, one material entry per storage. Storages are associated with actual
placed barn/yard/cart entity ids rather than disconnected stock records. The model-less yard is
retained as an entity, with its profile-defined 4×6 footprint and 6,000-unit capacity; world
rectangles use the placed dimensions after rotation. Distribution visits the ordinary registry,
excluding market owners. The citizen's carried
capacity and accepted flags come from its packaged `:storage` description. Fetch consumption
changes the source ledger that the Status Bar sums; equipping does not leave an extra item in
the citizen's carried inventory.

## Finding stored goods

`FUN_140093200` walks the town manager (`Game +0xf0`) registries in their existing order:

- market/radius owners at manager `+0x10`, length `+0x18`;
- ordinary StorageLocation owners at `+0x00`, length `+0x08`.

The query category is an **index**, not the raw-material flag mask:

| Category | Meaning | RawMaterialDescription `+0x5c` bit |
|---|---|---|
| 0 | food | 5 (`0x20`) |
| 1 | fuel | 6 (`0x40`) |
| 2 | tool | 9 (`0x200`) |
| 3 | herb | 13 (`0x2000`) |
| 4 | clothing | 14 (`0x4000`) |

`FUN_140094410` considers matching entries only while quantity − reserved > 0. The **last**
eligible entry with description byte `+0x58` set wins without a draw. Otherwise it draws
`Rand(eligible count)`, even for one entry, and selects that ordinal in ledger order. A rejected
storage mask can therefore still have consumed a material-selection draw.

For each radius owner the search obtains its entrance with `FUN_14008d480`, compares its path
rank to the home entrance against `RadiusDescription +0x28` (inclusive), and ranks its entrance
against the citizen. Only an entrance rank strictly below the current best triggers item
selection. `FUN_14004f710` then draws one use point, checks connectivity and returns its path
rank and world position. A strict improvement replaces the result. The covered-home flag is
sticky once an eligible material is found, even when that owner's final use point fails. After
**all** radius owners, a selected result plus that flag returns before ordinary storage is scanned.
It is not an immediate return on the first market, nor necessarily the covered market itself.

Otherwise ordinary owners undergo the same entrance-rank prefilter and material selection.
Their selected material's flags must intersect `StorageLocationDescription +0x20`; zero in
that field falls back to StorageComponent `+0x3c`. The packaged location serializer is
`FUN_140132940` (version 2). `FUN_140132aa0` supplies the final target:

- With InteractComponent, visit every use point in order, choosing the first reachable strict
  minimum by path rank. An existing component with zero points fails; it does not use the map
  rectangle fallback. No Random draws occur in this helper.
- Without it, scan the map rectangle grown by one cell, rows outer, restricted to mask `0xe07`
  and connectivity under `0xe1f`. The original includes an additional upper row: y is
  `rect.y - 1 ..= rect.y + height + 1`, whereas x stops before `rect.x + width + 1`.
  Strict minima retain the first cell; return its centre on the ground.

Path rank is the existing SectorGraph implementation. Negative ranks use
`map width + (int)sqrtf(dx² + dy²)` (`FUN_140008830`); slot targets still require connectivity.
The graph is marked when cell types change, rebuilt before ranking, and its cache ages once per
iteration before animations. `RadiusDescription`'s serializer `FUN_140055e90` stores the radius
integer then its display external reference. All 14 radius and 4 location descriptions parse.

## Fetch activities and actions

`FUN_140111a50` (tool, activity 8) and `FUN_140111830` (clothing, activity 3) first set their
75-second cooldown. Already-equipped citizens return false. Otherwise they compute the current
cell, obtain the home entrance when assigned, and search category 2 or 4. Failure clears tool
status `+0xa8` or clothing status `+0x1e0`, and sets citizen `+0x88 |= 4`. Tool failure also
sets StatusIconComponent bit 4; the live mask is ported, while its billboard renderer remains open.

On success `FUN_1400c2540` / `FUN_1400c2180` clear the character stack and install this execution
order (pushed in reverse):

1. MoveTo the returned target: walk clip 1, stand 0, mask `0xe1f`, cost mode 0, random speed
   `1 + RandFloatTol() × 0.1`.
2. PerformWork: face the source model (map-centre fallback, `FUN_1400c7e70`), clip 8,
   duration 1 second, without spending a tool use.
3. **ConsumeRawMaterial**, vtable `0x140166050`, constructor `FUN_1400cb880`, start
   `FUN_1400cb420`: consume one selected unit directly from source storage. This action
   reserves the unit during construction (`FUN_1400cb980`, builder stack argument is true).
   It is not PickupStoredRawMaterial. Reserved consumption releases its reservation and
   subtracts without rechecking available stock; unreserved consumption checks availability.
4. Send Citizen message 1 with `RawMaterialDescription +0x54` for tools, or message
   `6 + ((flags >> 15) & 1)` for clothing.
5. On empty, send CitizenAI message 0 to choose again with zero elapsed time.

Both activity success branches set citizen `+0x54 |= 4`, `+0xb0 = 0`, `+0x30 = -1`.
Message receiver `FUN_140117730` dispatches tool message 1 to `FUN_1401152b0`: add the message
amount to `+0xac`, set `+0xa8` to that amount, and clear StatusIconComponent bit 4. Clothing messages
6/7 set **both** `+0x1e0` and `+0x1e4` to 1/2, set `+0x88 |= 0x800`, and set `+0x1e8` to
`CitizenDescription +0x130 × 300` seconds in original microsecond ticks.

The distinct **PickupStoredRawMaterial** executor is also ported (`FUN_1400cf210`, vtable
`0x1401663a0`). Serializer `FUN_1400cf5a0` records action state `+0x18`, citizen `+0x20`, source
owner `+0x28`, material/count array `+0x30` with length `+0x38`, and partial flag `+0x40`.
A nonpositive requested count uses remaining carried capacity / unit weight. For each entry,
strict mode fails on insufficient total source quantity; partial mode clamps to total stock and
fails at zero. It releases the actual count's reservation, subtracts source stock and adds to
the carried ledger. Entries execute in order, so a later failure preserves earlier transfers.
`FUN_1400cf700` reserves and merges pickup requests; its general job builder is not wired to any
new activity in this slice. Abort `FUN_1400cf140` / consumption abort `FUN_1400cb300` release
pending reservations, clamping to zero. ActionStack clearing now invokes that cleanup even
when the reserved action has not started.

### Shared Random order

The fetch path uses only the game stream: home entrance first when present; each market entrance,
then eligible material selection and market use-point selection when the rank gate passes;
then each ordinary entrance, eligible material selection, and deterministic slot scan; finally
MoveTo's speed draw. `FUN_14008d480` retains its discarded `Rand(use-point count)` when the
count exceeds one, although entrance point zero is always returned. Ordinary slot scanning
consumes nothing; a market use-point selection draws even when there is only one point.

### Equipment verification and deviations

All local original-function outputs are under ignored `game-data/derived/emu/storage/`:

- `tools/emu/storage.py` → `storage_emu.rs`: 600 search rankings, masks, reservations, material
  preference, ties and next-Random checks. Geometry metrics are injected in these search cases.
- `storage_geometry.py` → `storage_emu.rs`: 240 entrance / nearest-slot / market queries on
  eight 32×32 maps, using original regions and sector graphs. Identity/point transforms and
  flat ground are injected; distance-cache writes are disabled. Ranks, targets and next Random
  match, including disconnected maps and empty interaction lists. A further 240 facing angles
  from `FUN_1400c7e70` match bit-for-bit: normalize the complete 3D vector, round double atan2
  to f32, then multiply by the original degrees constant.
- `storage_ledger.py` → `storage_emu.rs`: 400 add/remove/occupancy/consume/pickup cases,
  including shortages, reservations, partial transfer, remaining capacity and multi-entry failure.
  Historical consumption-counter updates and owner/type lookup are stubbed.
- `equipment.py` → `equipment_emu.rs`: 300 tool/clothing messages match fields, flags and timers;
  mesh-overlay toggles are stubbed.
- The seeded Medium-profile integration test equips all 15 citizens with clothing and all 10
  adults with tools, then observes loitering and zero outstanding reservations. Tools change
  40 → 30 and clothes 30 → 15. It also asserts all eight starting stock totals and the yard's
  dimensions/capacity, and checks exhausted storage's statuses/cooldowns and
  pending-action abort cleanup. This proves remake behavior, not original gameplay timing.

Final validation: `cargo test --workspace` passes 99 tests (two pre-existing old-build save tests
ignored); the release `wasm32-unknown-unknown` app build succeeds. The requested seed-1234,
22-second New Game capture was opened at `game-data/derived/captures/storage-fetch-22.png`.
`tools/triage/determinism.sh 22 1234` produces byte-identical frames. These PNGs and emulator
outputs remain ignored; no game assets or decompiled listings are committed.

Known deviations retained: AI every iteration instead of bucket quota (and after placed-entity
bucket work); loiter sets use creation order instead of object address; animation blending is
absent; starting structures do not stamp the map. Additional boundaries: StatusIconComponent
billboards are not rendered; job-driven tool use, clothing wear and historical consumption counters
are not connected; the general storage-pickup job builder, loose piles, ongoing production and
save/load remain open. Registrations currently come from New Game placed structures; completion/
destruction of later storage buildings is not implemented. The happiness registry remains empty.

Open: remaining activity builders and their job/building queries, general pickup/delivery jobs,
residence assignment, the remaining actions, animation blending and bridge ground picking.
