# Starting world: start point and natural resource seeding

This note follows `FUN_1400a87d0` (the New Game start-condition setup) through the start-point search
and the procedural rock/ore/tree pass `FUN_1400a6780`, down to every random number consumed. The
evidence is Ghidra pseudocode checked against disassembly. The start point is ported and
bit-exact under emulation (`crates/game/src/world/map.rs`, `tools/emu/start_point.py`). The seeding pass
below is a specification that has **not yet been verified**: it creates entities through the
component system, which the emulator cannot run. Verifying it needs a save made right after
starting a new game with the current build. Profile data is in
[START-CONDITION-PROFILES.md](START-CONDITION-PROFILES.md).

## Random number streams

`FUN_14009bfd0` creates the game-loop `System::Random` (game-data context `+0xd8`) as
`Random(seed)` with the New Game seed. The terrain generator uses its own `Random(seed)` in the
generation context. The starting world is therefore fully determined by the seed and the selected
profiles. In New Game order, the game-loop stream is consumed by:

1. `FUN_1400a7bf0` (start point): no random numbers.
2. `FUN_1400a7490` (profile structures): one `Rand(model count)` per placed structure, choosing
   its `ModelDescription` variant.
3. `FUN_1400a6780` (rock, ore and tree seeding): see below.
4. `FUN_1400a62a0` (households), `FUN_1400a5eb0` (deer), `FUN_1400a57c0`, then the crop, orchard
   and livestock picks in `FUN_1400a87d0`.

## Start point (`FUN_1400a7bf0`, `FUN_1400a7d50`) — verified

The search starts at the map centre `(n/2, n/2)` and tests rings of `4, 8, 16, …` points. Point
`i` of a ring is at angle `i · (360 / count)` degrees, using `Math::Scalar::SinCos` with the CRT
`sinf`/`cosf`. The candidate cell is `((int)(cos · d) + n/2, (int)(sin · d) + n/2)`. Each ring
increases `d` by `(3r) / 4` (integer division), where `r` is the profile's `+0x3c` (13 for every
shipped profile). A candidate is accepted when every cell with `dx² + dy² ≤ r²` is inside the
map and of type 2 (buildable ground).

## Seeding pass (`FUN_1400a6780`)

The arguments are the start cell `(sx, sy)` and `r²`. First the pass builds three Perlin fields
that it never uses, consuming six `RandTol(0x7ffffff)` calls. Then, for each template in the
profile's `+0x70` list (Rock, Ore, Tree, in that order), it reads the template's
`NaturalResourceDescription` (`_naturalresource.crs`, serializer `FUN_14012b2e0`):

| Offset | Rock | Ore | Tree | Use |
|---|---|---|---|---|
| `+0x28`, `+0x2c` | | | 1, 4 | placement noise persistence, octaves |
| `+0x30` | | | 30 | noise scale (sampled at `cell / scale`) |
| `+0x34` | | | 0.35 | minimum `noise·0.5 + 0.5` |
| `+0x38` | | | 0.4 | `RandFloat()` must be ≤ this |
| `+0x3c`, `+0x40` | | | 0, 1 | ground height range |
| `+0x44` | | | 0.75 | minimum normal z |
| `+0x48` | | | true | choose the model variant by noise clusters |
| `+0x4c`, `+0x50` | | | 1, 1 | cluster noise persistence, octaves |
| `+0x54`, `+0x58` | | | 5, 0.5 | cluster noise scale, threshold |
| `+0x5c` | | | 5 | 1-in-N chance of a random variant instead |
| `+0x60` | | | 0.4 | (used by the positioning helper) |

Only the Tree column has been decoded so far. The Rock and Ore values come from their own
`_naturalresource.crs` files.

For each template, in order:

1. If clustering is on, create one Perlin field per `ModelDescription` variant:
   `PerlinNoise(+0x4c, +0x50, b, a)`, where `a` then `b` are `RandTol(0x7ffffff)`.
2. Create the placement field `PerlinNoise(+0x28, +0x2c, b, a)` the same way.
3. For each map row `y`, then each column `x`, over `0..n`:
   - skip unless `+0x34 ≤ field(x · (1/+0x30), y · (1/+0x30)) · 0.5 + 0.5`;
   - skip unless `RandFloat() ≤ +0x38`. This roll happens only for cells that pass the noise test;
   - skip if the cell already holds an object (`+0x08`), or if `(y - sy)² + (x - sx)² < r²`;
   - cast the ground ray at the cell centre (see [MAP-CELLS.md](MAP-CELLS.md)). Skip if the
     height is below `+0x3c`, above `+0x40`, or the normal z is below `+0x44`;
   - if the template has a `GrowthDescription`, apply the density check `FUN_140126100`: count
     occupied cells among the 8 neighbours and those holding the same growth type. Skip if the
     same-type count exceeds growth `+0x5c` or the occupied count exceeds `+0x58`;
   - clone the template and run its components:
     - **positioning** (`FUN_14012ba10`, the natural-resource instance): three `RandFloatTol()`
       values for a jittered transform (the exact use is not yet traced). It records the cell and a 1×1 footprint;
     - **growth** (if present): `FUN_140126280(RandFloat())` sets the starting growth. That
       function draws one more `RandFloat()` for the growth timer;
     - registration with the entity manager (`FUN_1400c9d70`); the cell's object pointer is set
       during this step (inferred, not yet traced);
     - **spawn** (if present, `FUN_140056620`): for each spawn-table band containing the growth
       value, `(int)(growth / step) + 1` attempts of `FUN_1400560e0`. Each attempt draws
       `RandFloat()` against `1 / chance`. On success it draws two `RandTol(radius)` cell offsets,
       checks the spot (`FUN_1400a4aa0` and the density check), clones the spawned template (herb,
       mushroom and so on) and positions it with three more `RandFloatTol()`. Its growth then
       takes one `RandFloat()` in `FUN_140126280`, plus another `RandFloat()` when the spawn asks
       for random growth;
     - **model variant** (if clustering): pick the first variant `k` with
       `+0x58 ≤ cluster_k(x/+0x54, y/+0x54) · 0.5 + 0.5` (else 0). If `Rand(+0x5c) == 0`, replace
       it with `Rand(count)`. Then draw `Rand(sub-model count)` for the sub-model.

The cell occupancy written in each step feeds the object and density checks of later cells and
later templates, so the order above matters.

## Component setup, growth, spawn and stamping (traced for the port)

Constructing an entity (`FUN_1400c8bf0`) runs each component's setup (vtable slot `+0x20`) in
template order. For natural resources (map, model, naturalresource, growth, spawn) only two of them
draw from the stream:

- **model** (`FUN_140129f90`): `Rand(variant count)` only when the `ModelDescription`
  random-variant flag (`+0x38`) is set. This is true for Rock, Ore and the spawned herbs, false for
  Tree;
- **growth** (`FUN_140125bc0`): maximum growth = `RandFloatTol() · +0x24 + +0x20` when `+0x20 > 0`,
  otherwise −1. Tree: 5 ± 1.

A scan of every `Component` class's setup and activation slots (to call depth 2) found draws only
in Model, Growth, Population, Tombstone, Tornado and Animal.

The remaining steps:

- **Growth** (`FUN_140126280(g)`): the growth is `g · max`, stored at
  `NaturalResourceComponent +0x28`. The mesh-instance scale (`+0x134`) is `min(growth, 1)` on all
  axes. When `GrowthDescription +0x64` is set it is instead `(clamp(g/0.15), same, g, 1)`. The
  function then draws one `RandFloat()` for the timer, `· +0x50 · 300` s.
- **Stamping** (map activation `FUN_140129840` → `FUN_140129680` → `FUN_1400a30f0`): it runs only
  when the `MapDescription` stamp flag (`+0x30`) is set. Natural resources have an empty footprint
  and type 4, so their cell becomes type 4 and its `+0x08` points at the entity. Buildings (barn,
  houses) have this flag clear, so their map activation does not change the cells. They are
  built at once, though, and hold their footprint's cells before the deer population starts
  (*measured* on `Save2`: the town's herd picks its target as if the four population cells under
  them had no capacity).
- **Spawn** (`FUN_140056620`). For each entry whose `[lo, hi]` contains the parent's growth, it
  makes `(int)(growth / child +0x20) + 1` attempts of `FUN_1400560e0`. Each attempt:
  1. draws `RandFloat() ≤ 1 / child +0x48`;
  2. draws two `RandTol(child +0x4c)` offsets;
  3. checks the spot with `FUN_1400a4aa0(map, x, y, mask 4, 0.98, 1000, 0, …)`: type 2, no entity
     at `+0x00`, cell byte `+0x28` clear, normal z ≥ 0.98;
  4. runs the density check;
  5. constructs the child (setup draws), positions it (three `RandFloatTol()`), and calls growth
     with 0.0 (one timer draw);
  6. registers it;
  7. then `RandFloat()` and growth again (another timer draw).
- **Model variant with clusters:** the final mesh is `Rand(mesh count)`, passed to
  `FUN_14012a8a0(model, variant, mesh)`.
- **Position** (`FUN_14012ba10`): the first `RandFloatTol` jitters y and the second x, both
  `· +0x60`. Height is the ground at the cell centre. Yaw is `RandFloatTol · 180°`.
- **Start structures** (`FUN_1400a7490`). For each record:
  - construct the template;
  - take its size from `CreatePlacedDescription +0x44/+0x48`, or the record's two ints;
  - position it at the midpoint of the ground at its two corner cells;
  - set the map component rectangle and rotation;
  - for templates with a model, draw `Rand(variant count)` → `FUN_14012a8a0(model, v, 0)`;
  - register it.

  The yard has no model, so the Medium profile draws once (barn) and Easy draws seven times.

The new descriptions parse completely across the shipped data
(`crates/formats/tests/templates/`): `ComponentDescription`, `ModelDescription`,
`NaturalResourceDescription`, `GrowthDescription`, `SpawnDescription` and `MapDescription`.

## Port

`crates/game/src/nature/seeding.rs` ports the structure draws and the full pass, including spawning.
`crates/sim/src/town.rs` runs it at New Game on the game stream `Random(seed)` before the
environment draws, then draws the placed models.

Start structures are drawn at their footprint centre with the finished mesh. Creation runs with
`Game +0xd0` bit 0 set: the build component (`FUN_14010cfb0`) then selects mesh `count − 1`
instead of the first construction stage, and the clear-area component (`FUN_14011ce30` →
`FUN_14011d230`) clears entities under the footprint. Sizes come from `CreatePlacedDescription`
(`FUN_14011eda0`: u32, u32 list, footprint string, width `+0x44`, height `+0x48`, `+0x40`,
external). The barn is 8×5.

Deviations and assumptions:

- The structures become built sites (`StartingTown::build_structures`): their maps are stamped
  and zones taken at once, and their state machines enter state 2 on activation (the original
  runs states 0, 1 and 8 instantly under the flag). Their decals are not drawn.
- *Inferred:* the model transform's offset is the computed footprint centre. The decompiled call
  passes a stack vector the decompiler did not resolve.
- The density check uses cached external growth-description identities in place of original
  pointers; primary occupancy and the placement flag are retained as distinct cell state.

Seeded maxima, timers and cell state now survive into the scheduled growth loop, periodic
same-template offspring and visual smoothing; see [GAME-LOOP-STEP-18.md](GAME-LOOP-STEP-18.md).

**Ongoing spawning** (SpawnComponent's scheduled update, `FUN_140056430`;
`Seeder::scheduled_spawn`). It runs on each bucket visit, after the GrowthComponent (the
template's component order). Each entry whose `[lo, hi]` holds the parent's growth advances its
own clock by the elapsed time; clocks outside their band stand still. Every child growth period
(`child +0x28 × 300` seconds) it makes one `FUN_1400560e0` attempt without the random growth,
and none while `Game +0xd0` bit 10 is set. The trees use this for herbs, onions, blueberries,
mushrooms and roots. A child goes in the parent template's bucket.

## Open items

- Verify the port entity for entity against a save made right after New Game.
- Confirm `FUN_1400a4aa0` (spawn spot check) and the exact transform maths in `FUN_14012ba10`.
- Verify against a save made right after New Game with the current build. That save holds every
  initial entity and its transform.
