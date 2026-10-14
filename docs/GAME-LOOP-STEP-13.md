# Game loop step 13: placement checks and resource transition dispatch

This step closes the helpers left open by [GAME-LOOP-STEP-12.md](GAME-LOOP-STEP-12.md). The evidence
is static Ghidra pseudocode for the Steam x64 executable, checked against the original executable
disassembly for the placement helper's output arguments. The meanings of several serialized fields
remain unknown, so this note records branch conditions and effects rather than labels such as
"mature" or "harvested".

## Candidate cell admission

The periodic growth path calls `FUN_1400a4aa0` with type mask `4`, minimum surface-normal z
`DAT_140178840`, height tolerance `DAT_140178948`, reference height zero, and both occupancy
overrides disabled. The helper rejects out-of-bounds coordinates, then checks the candidate map
cell:

- Its cell type at `+0x29` must select a bit present in the supplied type mask. With mask `4`, the
  accepted encoded type is `2`.
- Unless the occupancy override is enabled, the entity pointer at `+0x00` must be null. A non-null
  `setOwner` argument permits the existing entity only when its `+0x28` owner pointer equals that
  argument; this growth caller passes null.
- Unless the cell-flag override is enabled, the byte at `+0x28` must be zero. This caller also passes
  false for that override.
- `FUN_1400a48b0` supplies the surface normal and ground height. The helper requires normal z at
  least the minimum and absolute ground-height difference from the reference no greater than the
  tolerance.

Only then does the caller run the eight-neighbor density check and clone/register the child resource.
The cell record layout and byte offsets are cross-referenced in [MAP-CELLS.md](MAP-CELLS.md). The
numeric meaning of the `+0x28` cell byte is not assigned here.

## Threshold transition on a natural resource

The GrowthComponent holds a linked `NaturalResourceComponent` at `+0x28`. When its configured
instance threshold is crossed, `FUN_140125520` updates the cell's map byte, writes the owning entity
pointer into the map cell's entity slot at `+0x10`, clears the linked component's integer at `+0x28`
and byte at `+0x2d`, then calls `FUN_14012bc50`. It subsequently removes the GrowthComponent's
scheduled and smoothing capability bits (`0x08` and `0x10`).

`FUN_14012bc50` sets flag `0x200000` on the linked resource component and calls
`FUN_14012b910(component, 2)`. That dispatch depends on the linked `NaturalResourceDescription`
byte at `+0x64`:

- If the byte is nonzero, the function finds the owner's mesh-animation component and calls
  `FUN_14012a5c0` with animation index `2`, start time zero, end time zero and the executable's unit
  time constant. It then sets animation-component flag `0x04`.
- If the byte is zero and the component has flag `0x200000`, it calls `FUN_14012b5c0` with a null
  replacement pointer. That helper queues the current owner for removal if it is still registered.
  When the resource component's byte `+0x2d` is set, it can also clone an entity from the
  `NaturalResourceDescription` reference at `+0x20`. The amount comes from the referenced resource's
  integer at `+0x40` or `+0x44`, selected by component byte `+0x2c`, and is scaled by the component
  float at `+0x28` capped at one. It registers the clone only when the resulting amount is positive.

The executable establishes these conditions and effects, but not whether the branches mean a
particular gameplay concept such as maturity, harvest, depletion, or regrowth. That mapping needs
resource-description and saved-world comparisons before the port assigns names to the state.

## Relation to the port

The current port's natural-resource seeding does not yet apply this periodic placement admission or
threshold dispatch. A faithful implementation needs the cell type, occupancy, surface and height
checks in the same rejection order, followed by the density check. It must also preserve the
distinct animation, queued-owner-removal and optional entity-clone branches without collapsing them
into a generic growth completion event.

## Evidence pointers

- `analysis/export/Application-steam-x64.exe/decomp.c`: `FUN_1400a4aa0` (`0x1400a4aa0`) checks
  candidate cells and surface conditions; `FUN_140125070` (`0x140125070`) calls it with the periodic
  spawn parameters; `FUN_140126100` (`0x140126100`) applies neighborhood limits.
- `FUN_140125520` (`0x140125520`) performs the threshold transition; `FUN_14012bc50`
  (`0x14012bc50`) sets linked-component state and dispatches index 2; `FUN_14012b910`
  (`0x14012b910`) selects animation or the alternate branch; `FUN_14012b5c0` (`0x14012b5c0`)
  queues/remakes entities; `FUN_14012a5c0` (`0x14012a5c0`) requests mesh animation.
- [MAP-CELLS.md](MAP-CELLS.md) records the serialized cell fields; [RESOURCE-SEEDING.md](RESOURCE-SEEDING.md)
  describes initial resource setup.
- [GAME-LOOP-STEP-12.md](GAME-LOOP-STEP-12.md) traces the timed spawn chance, child creation and
  eight-neighbor density limit.

## Next trace

Correlate the description fields at `+0x40`, `+0x44`, `+0x64` and the component byte at `+0x2d`
against decoded resource templates and saved-game instances. The shipped description split is
documented in [GAME-LOOP-STEP-14.md](GAME-LOOP-STEP-14.md). Then compare cell byte `+0x28` and the
growth-updated byte `+0x2c` before and after the transition to establish their meanings.
