# Starting world: start point and natural resource seeding

This note follows `FUN_1400a87d0` (the New Game start-condition setup) through the start-point search
and the procedural rock/ore/tree pass `FUN_1400a6780`, down to every random number consumed. The
evidence is Ghidra pseudocode checked against disassembly. The start point is ported and
bit-exact under emulation (`crates/game/src/map.rs`, `tools/emu/start_point.py`). The seeding pass
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

## Open items

- Decode the Rock and Ore `NaturalResourceDescription` values and the spawn, growth and model
  descriptions, then port the pass.
- Confirm `FUN_1400a4aa0` (spawn spot check) and the exact transform maths in `FUN_14012ba10`.
- Verify against a save made right after New Game with the current build. That save holds every
  initial entity and its transform.
