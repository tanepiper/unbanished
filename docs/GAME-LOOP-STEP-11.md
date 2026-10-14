# Game loop step 11: scheduled natural-resource growth

This step follows a concrete component through both entity-manager update phases. Evidence is
static Ghidra pseudocode for the Steam x64 executable and serialized game resources. The timing
relationship comes from the call structure; no live timing capture was made.

## Growth component in the scheduled tick

`GrowthComponent` is registered at `0x140154f50`. Its constructor `FUN_140125dc0` sets component
capability flags to `0xCB`, which includes scheduled-update bit `0x08`. In the five-bucket pass,
`FUN_1400c9560` dispatches components with bit `0x08` through virtual slot `+0x48`. The
`GrowthComponent` vtable at `0x1401699c0` maps that slot to `FUN_140125520`.

`FUN_140125520` advances the growth value stored at the linked growth object `+0x28`, using the
delta passed by the entity scheduler. Its base rate is inversely proportional to the growth
description float at `+0x28`, scaled by the executable constant `DAT_140178934`. When the description
boolean at `+0x2C` is set, the current season value (environment state at `+0x18`) and description
fields `+0x30/+0x34` produce a seasonal weight: a parabolic `1 - t²` within the configured interval,
zero outside it, with a separate constant-valued branch below `DAT_1401788d0`. When the flag is
clear, the seasonal multiplier is 1. The exact C++ field names and units represented by these
offsets/constants are not available in the export.

After changing the linked growth value, the component sets its capability bit `0x10` and calls
`FUN_1400c8940` to recompute the owning entity's aggregate capabilities. This queues the entity for
the manager's bit-`0x10` set, independently of the five scheduled buckets.

## Per-frame follow-up pass

After the configured simulation iterations, `FUN_14009b850` calls `FUN_1400c98a0` once. That
manager pass visits its bit-`0x10` entities; `FUN_1400c8140` dispatches components carrying bit
`0x10` through virtual slot `+0x50`. The GrowthComponent vtable maps this slot to `FUN_140125370`.

`FUN_140125370` moves a stored value at component `+0x48` toward the current growth value, using an
exponential interpolation when the difference exceeds a small constant and snapping when it does
not. It writes the result into linked-object fields `+0x134..+0x140` (with a separate normalized
case when a byte in the growth description is set). Once stable, it clears capability bit `0x10`
and recomputes the owner mask, removing the entity from this follow-up set. The downstream consumer
of those linked-object fields has not been traced, so they are described as smoothed output rather
than assigned a visual meaning.

## Pause and speed relationship

The call order gives two distinct cadences:

- Each active simulation iteration calls `FUN_1400c9560`, so growth advancement repeats with the
  game's 1x/2x/5x/10x simulation iteration count.
- The `FUN_1400c98a0` follow-up runs once after that loop, not once per simulation iteration. When
  simulation speed is paused and the iteration count is zero, this follow-up still runs.
- If `UserInterface::PausesGame` is true, `FUN_14009b850` skips both the scheduled simulation loop
  and the follow-up pass, then continues through UI/audio work.

Thus the separate smoothing pass follows rendered game updates while the game is active, even when
simulation speed is set to zero. It is not a fixed calendar interval.

## Relation to the port

Natural resources are seeded as entities with `GrowthDescription` data; see
[RESOURCE-SEEDING.md](RESOURCE-SEEDING.md). The current new-game port does not reproduce this
growth component loop. This trace provides a concrete next behavior boundary: grow the resource's
state during simulation ticks, then publish/smooth its derived values once per active game update.
It does not yet cover growth-triggered spawning, resource removal, or every interaction with the
map's natural-resource fields.

## Evidence pointers

- `analysis/export/Application-steam-x64.exe/decomp.c`: `FUN_14009b850` (`0x14009b850`) orders the
  per-iteration scheduler and once-after-loop set pass; `FUN_1400c9560` (`0x1400c9560`) dispatches
  scheduled components; `FUN_1400c98a0` / `FUN_1400c8140` (`0x1400c98a0`, `0x1400c8140`) dispatch
  bit-`0x10` components.
- `FUN_140125dc0` (`0x140125dc0`): GrowthComponent flags; `FUN_140125520` (`0x140125520`): scheduled
  growth and seasonal weighting; `FUN_140125370` (`0x140125370`): smoothing and bit clear.
- GrowthComponent type registration at `0x140154f50`; vtable `0x1401699c0`, slots `+0x48` and
  `+0x50`.
- [ENTITY-COMPONENT-LIFECYCLE.md](ENTITY-COMPONENT-LIFECYCLE.md): scheduler, capability sets and
  component construction.
- [RESOURCE-SEEDING.md](RESOURCE-SEEDING.md): natural-resource templates and initial growth.

## Next trace

Follow `FUN_140125520`'s growth thresholds into `FUN_140125070`, the natural-resource map update,
and any spawned-template registration. Preserve the distinction between the target growth value,
the smoothed linked-object fields, and the map's discrete occupancy state.
