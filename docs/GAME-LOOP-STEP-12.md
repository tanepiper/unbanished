# Game loop step 12: growth-triggered natural-resource spawning

This step follows the two branches that leave the regular GrowthComponent update: periodic nearby
entity creation and a growth-threshold transition that updates the map. Evidence is static Ghidra
pseudocode for the Steam x64 executable and the serialized GrowthDescription layout. Semantic field
names remain provisional where the code only exposes offsets.

## Periodic child placement

In `FUN_140125520`, the periodic branch is enabled by GrowthDescription byte `+0x44`. It waits until
the growth value reaches the float at `+0x54`, accumulates time at GrowthComponent `+0x40`, and calls
`FUN_140125070` for every elapsed interval `GrowthDescription[+0x50] * DAT_140178934`, retaining the
remainder in the timer.

Each `FUN_140125070` call:

1. Draws a random float and proceeds only when it is at most `1 / GrowthDescription[+0x48]`.
2. Chooses an x/y offset with `RandTol(GrowthDescription[+0x4c])` from the parent entity's cell.
3. Rejects out-of-bounds cells, then calls `FUN_1400a4aa0` for the placement checks and
   `FUN_140126100` for the natural-resource neighborhood limits.
4. If valid, clones the parent's `ComponentDescription` through `FUN_1400c8bf0`, positions the new
   entity at the selected map cell, initializes its `GrowthComponent`, chooses a model variant, and
   registers it with the entity manager using the parent template's bucket at `+0x20`.

`FUN_140126100` examines the eight neighbors around the candidate cell. It counts occupied cells and
the subset whose `GrowthComponent` references the same GrowthDescription. It admits the candidate
only when those counts do not exceed that description's integer limits at `+0x58` and `+0x5c`.
This is the concrete density gate used both by this periodic path and natural-resource seeding.

## Map updates at a growth transition

`FUN_140125940` converts the entity's signed map coordinates into a cell index and adjusts the byte
at cell-record `+0x2c`. A nonnegative input scales the squared prior byte fraction; a negative input
adds its magnitude. The result is scaled by `DAT_14017892c` and clamped to a byte. The pseudocode
does not name this map byte, so this note does not assign it a visual or gameplay label.

When the GrowthComponent's instance threshold at `+0x4c` is positive and crossed by the updated
growth value, `FUN_140125520` also writes the owner's external ComponentDescription reference
into the map's per-cell table (cell record `+0x10`). It adjusts the cell byte through `FUN_140125940`, clears growth-instance
fields, calls `FUN_14012bc50` on the linked growth object, and clears component capability bits
`0x08` and `0x10` through `FUN_14011de20`. The same cleanup shape appears when growth would fall
below zero and the linked natural resource's allow-negative capability flag is clear. These are terminal transitions out
of the scheduled and smoothing passes; whether the entity is now mature, dormant, or being removed
depends on the branch and is not established by these calls alone.

## Relation to the port

The port seeds natural-resource entities but does not yet implement periodic self-seeding, the
eight-neighbor density gates, or these map-cell updates. A faithful implementation needs to preserve
the random draw order, candidate rejection order, template bucket, and distinction between growth
fraction and the map's discrete per-cell state.

## Evidence pointers

- `analysis/export/Application-steam-x64.exe/decomp.c`: `FUN_140125520` (`0x140125520`) triggers
  timed attempts and terminal transitions; `FUN_140125070` (`0x140125070`) places a child resource;
  `FUN_140126100` (`0x140126100`) applies neighbor limits; `FUN_140125940` (`0x140125940`) adjusts
  the map byte; `FUN_140126280` (`0x140126280`) initializes the child's growth state.
- `FUN_1400a4aa0` performs the candidate placement checks; `FUN_1400c8bf0` clones an entity from a
  `ComponentDescription`; `FUN_1400c9d70` registers it with the entity manager.
- `FUN_140125e50` (`0x140125e50`) serializes GrowthDescription fields; constructor defaults and
  resource seeding are described in [RESOURCE-SEEDING.md](RESOURCE-SEEDING.md).
- [GAME-LOOP-STEP-11.md](GAME-LOOP-STEP-11.md): scheduled growth and the follow-up smoothing pass.

## Next trace

Placement cell tests and the natural-resource linked-component transition are traced in
[GAME-LOOP-STEP-13.md](GAME-LOOP-STEP-13.md). Compare the map bytes against an actual seeded save
to distinguish the cell state at `+0x28` from the growth-updated byte at `+0x2c`.
