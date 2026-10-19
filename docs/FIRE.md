# Fire, firefighting and wells

Port: `crates/game/src/disasters/fire.rs` (with the damage and rubble in `destroy.rs`, see
[JOBS.md](JOBS.md#demolition-cratesgamesrcdestroyrs)). Test: `a_house_burns_and_is_rebuilt`
(`apps/game/src/tools/placement_tool.rs`).

## FireDescription

`Template\Common.rsc:fire*` (serializer `FUN_14004a560`; the building's ComponentDescription
lists it like its own descriptions). Fields in serialized order:

| Offset | Meaning | WoodHouse | StoneHouse | WoodBuilding | StoneBuilding | Production |
|---|---|---|---|---|---|---|
| `+0x20` | burn seconds | 1 | 1 | 1 | 1 | 1 |
| `+0x24` | burn, × 300 s | 0.5 | 0.5 | 0.5 | 0.5 | 0.5 |
| `+0x28` | ParticleDescription slot | 0 | 0 | 0 | 0 | 0 |
| `+0x2c` | ambient emitter | 0 | 0 | 0 | 0 | 0 |
| `+0x30` | spread interval, × 300 s | 0.033 | 0.033 | 0.033 | 0.033 | 0.033 |
| `+0x34` | odds of catching from a neighbour | 0.5 | 0.33 | 0.5 | 0.33 | 0.5 |
| `+0x3c` | buckets that put it out | 100 | 100 | 100 | 100 | 100 |
| `+0x44` | ignition years | 20 | 30 | 0 | 0 | 40 |
| `+0x48` | ignition odds | 100 | 100 | 0 | 0 | 100 |
| `+0x38` | emergency radius (cells) | 20 | 20 | 20 | 20 | 20 |
| `+0x40` | most firefighters | 50 | 50 | 50 | 50 | 50 |

With odds 0 a building never catches fire by itself (`FUN_1400ae760` returns false without a
draw) but can still catch from a neighbour.

## FireComponent

Vtable `0x14015f480`, 0x48 bytes (constructor `FUN_14004a4e0`, serializer `FUN_14004a3f0`):
`+0x28` buckets thrown, `+0x2c` buckets at the last spread, `+0x30` burn left, `+0x38` towards
the next spread, `+0x40` towards the next ignition check.

**Scheduled update** (`FUN_140049910`, every visit of the entity's bucket; the port runs it every
iteration like the other site updates): not burning, and fires not off (`Game +0xd0` bit 6),
`+0x40 += dt` and `FUN_1400ae760(+0x40, [+0x44, +0x48])`: `RandFloat < seconds /
(+0x44 × 12 × 300 × +0x48)`, the time restarting at each check; a hit ignites. Burning,
`+0x30` and `+0x38` count down; with burn left and the spread due, the spread runs; with no burn
left, PutOut.

**Ignite** (`FUN_140049f70`): not for a building damaged (6, 7) or being torn down (3..5). The
burn is `+0x24 × 300 + +0x20` seconds; the DestroyComponent is damaged for half of it
(`FUN_140123c80`, which refuses a site not started). Then `+0x30` = the burn, `+0x38` =
`+0x30` *seconds* (not × 300), the buckets 0; the particles start; a FireWork (`FUN_1400ef0a0`
with `+0x40`) is registered with mask `0x4000` and prioritised (`Jobs::Prioritize`); when no
other fire burns (`Map +0x9c` < 1) the event log gets type 31 at severity 3 (colour
`0xffff6060`) and, with AutoSlow on (settings `+0x17`), the speed control is asked to slow; the
fire sound plays at the model; the emergency layer is raised (`FUN_14004a320(+1)`).

**Emergency layer** (`FUN_14004a320` → `FUN_1400a4f50`): map cell byte `+0x22` (layer 6) `±1`
over the map rectangle grown by `+0x38` on every side (clipped to the map), and the layer total
`Map +0x9c`.

**Bucket** (message 0, `FUN_1400499f0`): `+0x28 += 1`, the burn loses `+0x30 × 300 / +0x3c`
seconds; out of burn or with `+0x3c` buckets thrown, PutOut.

**PutOut** (`FUN_140049e20`): burn and spread 0, particles off, the DestroyComponent goes
straight to rubble (`FUN_140123ee0`, state 7), the work goes, the sound stops, the emergency
layer drops (`FUN_14004a320(-1)`). A fire always ends in rubble.

**Spread** (`FUN_140049a90`): `m = min(w, h)`; columns `max(x − m, 0) ..= min(x + w − 1 + m,
n − 1)`, rows `y − m ..= min(y + h − 1 + m, n − 1)` (the top row is not clamped). While the
spread is due: `+0x38 += +0x30 × 300`, `Rand(x0, x1)`, `Rand(y0, y1)`; the cell's occupant
(`+0x00`) with a FireComponent catches with its own `+0x34`, halved when the sky's state is
precipitation (environment `+0x10` = 0) and halved again when more than `+0x3c / 5` (the
target's) buckets were thrown here since the last spread, against `RandFloat`. Then `+0x2c =
+0x28`.

## FireWork

Vtable `0x1401676c0` (constructor `FUN_1400ef0a0`: `+0x48` most workers, `+0x50` the worker set),
profession slot `+0x50` the laborer (`Game +0xb0 → +0x30`).

- `+0x20` (`FUN_1400eeaf0`): at the most workers, 1. The water nearest the citizen
  (`FUN_14008d6a0`, within CitizenDescription `+0xf8`); the burning building's use point nearest
  that water (`FUN_140127fd0(interact, 0, ..)`: reachable with `0xe1f`, by path rank, below
  `0xffff`); `Work::PatienceCheck`; then the bucket run, the citizen's job bits, the worker set; 2.
  Nothing found: 0.
- `+0x30` / `+0x38`: the worker leaves the set. `+0x40`: every worker's stack is cleared.
  `+0x48`: workers no longer holding the task leave. `+0x58`: busy while it has workers.

**The bucket run** (`FUN_1400bfec0`, after clearing the stack, on empty the CitizenAI message 0):
MoveTo the water (walk 1, flags `0xe1f`, speed 3.5) → PerformWork clip 6 facing the water
(0.75 s) → ShowTool(8) → PerformWork clip 7 → MoveTo the fire's point (walk 4, speed 3.5) →
PerformWork clip 6 facing the building → message 0 to the FireComponent → CompleteTask(work, 0)
→ ShowTool(−1) → PerformWork clip 7.

**Who fights**: `Citizen::HasEmergency` (`FUN_14011b0b0`) is true when layer 6 is set over the
home's or the workplace's map-rectangle centre, or the citizen's own cell. The AI then takes
activity 2 (emergency work) at weight 7, re-choosing at once while not eating or warming, and
the job manager offers only category 14 (`AssignEmergency`, ignoring the workplace).

## Water sources

The town manager keeps `+0x130` the shore points, `+0xe0` the WellComponents and `+0x140` one
list per 8 × 8 block holding point indices (wells as `−1 − i`).

- **Shore points**: the map's setup (`FUN_1400a43f0`) files every ground cell (type 2) with a
  river or lake neighbour (types 7, 8; the first by rows) as (cell, water cell)
  (`FUN_14008e050`).
- **Wells**: WellComponent (vtable `0x140160fb0`, table `FUN_14005da70`): state 2 enter
  `FUN_14005d880` → `FUN_140097180` (the first empty slot, else a new one, filed under the well's
  map cell); state 3 enter `FUN_14005d840` → `FUN_140097070` (out of its block, the slot
  emptied). WellDescription has no fields.
- **Search** (`FUN_14008d6a0(cell, max)`): rings of blocks around the citizen's, ring `r` while
  the best so far exceeds `8 × max(r − 1, 0)`; per ring the left and right columns by rows, then
  the top and bottom rows by columns (`FUN_14008dc00`). An entry counts when its block offset
  `(8·dx)² + (8·dy)²` is within the best squared: a shore point whose cell is walkable (mask
  `0xe07`), or a well's use point 0; reachable with `0xe07`; ranked by path rank, else `n +
  distance`; strictly better replaces. A shore point gives (shore ground, water ground); a well a
  random use point (`Rand(count)`) and its entrance.

## Disasters

`FUN_14009d530` starts `Game +0xd0` at `0xf0` when the New Game's Disasters combo is not its
first item (bit 5 stops blight, bit 6 fires), else 0. The debug console's check boxes set the
same bits (`FUN_140060620`).

## Not ported

The event's formatted text beyond the toolbar name. (The particles are in
[PARTICLES.md](PARTICLES.md), the sound in [AUDIO.md](AUDIO.md), and the damaged material and
decal in [JOBS.md](JOBS.md#demolition-cratesgamesrcdestroyrs) and [DECALS.md](DECALS.md).) (The tornado, the other caller of the damage, is in
[TORNADO.md](TORNADO.md); AutoSlow comes from the event.)
