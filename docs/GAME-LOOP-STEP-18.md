# Game loop step 18: scheduled growth and live resource instances

This slice connects GrowthComponent to the Rust gameplay loop. Evidence is the Steam 1.0.7
Build 170910 x64 executable and Runtime DLL, local Ghidra pseudocode, focused SSE disassembly,
and execution of the original functions under Unicorn. The generated-town GPU test and native
capture execute the remake. No original-game runtime transition or save comparison was captured.

## Scheduler quotas and elapsed time

`FUN_1400c9460` resolves the previously unknown bucket quotas:

| Bucket | Quota field | Entities per simulation iteration |
|---|---|---:|
| 0 | `+0xa8` | 50 |
| 1 | `+0xac` | 15 |
| 2 | `+0xb0` | 8 |
| 3 | `+0xb4` | 5 |
| 4 | `+0xb8` | 2 |

`FUN_1400c9560` advances the clock and visits at most one revolution or the quota from each
bucket's saved cursor (`+0xbc..+0xcc`). Out-of-range cursors reset to zero. Zero-elapsed visits
skip callbacks but still move the cursor. Each entity receives the time since its own last visit;
quota throttling therefore does not lose elapsed growth time.

The bucket length is captured before callbacks. Children registered into that bucket wait until
a later iteration; `FUN_1400c9d70` sets their last timestamp to the current manager time. Removal
requests drain after all five buckets in request order. `FUN_1400c8db0` swaps the final entry into
the deleted slot without adjusting the cursor. Bucket 6 remains unscheduled. These operations are
ported in `crates/game/src/scheduler.rs`.

Runtime Time construction (`0x1800078c0`) divides f32 seconds by f64 seconds-per-tick and truncates
to integer ticks. AsSeconds (`0x180007900`) multiplies ticks by that f64 value, then converts to
f32. The initial step-18 port assumed nanosecond ticks. That interpretation is superseded by
[step 19](GAME-LOOP-STEP-19.md): Timer::Startup establishes fixed microsecond units. Rust and
all growth fixtures have been corrected and regenerated at that original frequency.

## Corrected growth inputs and transitions

`FUN_140125520` reads **temperature** at Environment `+0x18`, rather than a renderer season value.
The seasonal multiplier is a parabola around description `+0x30`, divided by `+0x34`. Outside
the window it is zero, except at temperature <= 32 where it is **-10** (`DAT_14017898c`). Growth
adds elapsed seconds times `1 / (description[+0x28] * 300)` times that multiplier, preserving
the original SSE multiply/add order in `crates/game/src/growth.rs`.

Interval and maximum tests are strict. Growth must exceed `+0x54` before accumulating the timer;
each spawn interval is consumed only when timer seconds exceed `+0x50 * 300`. Subtraction uses
f32, with the remainder converted back to integer Time. Exceeding instance maximum `+0x4c`, or
negative growth without natural-resource flag `0x100000`, invokes the map transition and clears
scheduled/smoothing bits. Game debug bit 10 suppresses spawning and the maximum transition, but
does not suppress growth or the negative-growth transition.

`FUN_140125940` changes map byte `+0x2c` using the squared old byte fraction for a nonnegative
argument, or adds a negative argument's magnitude, then truncates and clamps to `[0,255]`.
Its original gameplay name remains unknown. `FUN_14001b3f0` initializes it to zero. Map cell
`+0x10` stores the owner's external **ComponentDescription** reference, not an entity pointer;
the earlier step-12/13 wording is corrected accordingly.

The terminal path clears growth and the material-output guard byte before `FUN_14012bc50`.
Non-tree resources queue removal through `FUN_14012b910` without producing material. The port
removes them and restores their natural cell type. Tree resources request mesh animation index 2.
**Deviation:** Rust records `pending_animation` and preserves their mesh until that animation
controller is ported; it does not invent a harvest yield or tree-deletion timer.

## Periodic children and visible growth

`FUN_140125070` differs from startup spawning: draw chance and x/y offsets, reject bounds/cell/
occupancy/flag/surface/density failures, construct the parent's template, position it with three
jitter/yaw draws, initialize growth at **zero** with one timer draw, copy the parent's model
variant, draw a mesh within that variant, and register the child using the parent's bucket.
No random starting-growth fraction is drawn. Disassembly at `0x140125296` supplies the zero
float argument missing from Ghidra's call signature.

`Seeder::periodic_spawn` preserves this draw order. Cached external GrowthDescription identities
replace pointer comparison in the density gate, including descriptions shared by templates.
Primary occupancy at cell `+0x00` remains distinct from stamped objects at `+0x08`. Stamp
replacement queues the previous entity's removal (`FUN_1400a30f0`); removal uses
`FUN_140125a30`'s choice between description `+0x3c` and `+0x40` for map adjustment.

`FUN_140125370` clamps the visible target to `[description[+0x60], 1]`. Differences >= 0.001 blend
old value with 0.99 and target with the exact stored f32 **0.009999990463256836**; smaller
differences snap and clear smoothing membership. Uniform and tall scale shapes retain the 0.15
width divisor. Gameplay runs this once after its 1/2/5/10 simulation iterations, including toolbar
pause. StartingTown retains seeded cells, randomized maxima and timers. Instance-buffer updates
keep meshes, textures and terrain cached; a new mesh group uses the ordinary upload path.

## Verification and limits

`tools/emu/growth.py` writes ignored ground truth under `game-data/derived/emu/growth/`:

- **15,360** original growth/interval/transition/smoothing cases: positive/negative rates,
  seasonal and nonseasonal descriptions, temperatures inside/outside the window and below/at
  freezing, exact thresholds, maximum/negative branches, both scale shapes and debug/negative flags.
- **480** initialization cases: 20 RNG seeds, three maxima, four fractions and both scale shapes.
- **640** scheduler visits: eight iterations with every bucket longer than its quota.

All three `growth_emu` tests matched exactly, including f32 bits and integer timer values. The
emulator stubs entity creation, rendering, owner notifications and terminal resource dispatch;
growth, smoothing, Time conversion, map-byte adjustment and bucket visits execute in the original
binaries. This does not prove full-world RNG, placement or animation parity.

Rust tests verify child draw order, rejection before constructor draws, newborn scheduling,
deferred removal and restored occupancy. The app test generates a real Medium town, verifies ten
iterations with one smoothing step, verifies pause freezes calendar/growth while smoothing
continues, updates an already-uploaded GPU instance buffer and captures a nonempty rendered frame.

`cargo test --release` passed across the workspace with two pre-existing save tests ignored.
The release WASM build passed. Native capture `game-data/derived/captures/game-loop-growth.png`
was generated with `--boot-capture ... 10 --press buttonNew@4.5 --press buttonOk@5.0` and opened:
the map, starting barn, citizen models, trees/rocks and packaged toolbar are visible. The presses
found their controls. This is not an interactive browser-camera or original-game visual comparison.

## Remaining work

- Tree animation index 2 and its completion/removal chain are now ported in
  [step 19](GAME-LOOP-STEP-19.md); spatial audio dispatch remains open.
- Port ongoing SpawnComponent cross-template spawning and other component/manager phases.
- Add citizen AI/action scheduling, movement, needs and production/storage transfers. Citizens
  are not yet registered in this placed-entity scheduler and their starting models remain static.
- Compare initial and advanced worlds against original saves. Unported managers consume the shared
  RNG too, so the complete world stream is not yet original-equivalent.
- Replace retained dead-resource slots with recyclable handles after the remaining pointer/reference
  lifecycle is ported. Current handles remain stable while scheduler bucket entries swap.

Related: [step 11](GAME-LOOP-STEP-11.md), [step 12](GAME-LOOP-STEP-12.md),
[step 13](GAME-LOOP-STEP-13.md), [resource seeding](RESOURCE-SEEDING.md),
[entity lifecycle](ENTITY-COMPONENT-LIFECYCLE.md).
