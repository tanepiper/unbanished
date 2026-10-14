# Game loop step 16: natural-resource actions and animation completion

This step traces the natural-resource action methods and how an animation transition continues
through the entity manager. The evidence is static Ghidra pseudocode, class registration data and
the original executable's vtable entries. The higher-level action executor and its cadence are not
established.

## Description-selected transition

`FUN_14012bc50` sets NaturalResourceComponent flag `0x200000` and calls
`FUN_14012b910(component, 2)`.

- For descriptions with byte `+0x64` set, `FUN_14012b910` requests mesh animation index `2` through
  `FUN_14012a5c0`, then sets the component's capability bit `0x04` and recomputes the owner's
  aggregate capability mask.
- For descriptions with byte `+0x64` clear, the function calls
  `FUN_14012b5c0(component, null, false)` immediately when flag `0x200000` is set.

The first route waits for a component callback; the second enters the shared transition helper
directly.

## Scheduled animation completion

The NaturalResourceComponent registration names its vtable and allocator. Vtable slot `+0x40`
contains `FUN_14012b110`, the component callback for capability bit `0x04`. The entity-manager
capability pass calls this slot through `FUN_1400c82b0`.

`FUN_14012b110` checks `MeshInstance::IsAnimating` on the owning entity's linked model. While it is
animating, this callback makes no state change. Once animation has ended, it clears component bit
`0x04`, recomputes the owner's capability mask and, if component flag `0x200000` is set, calls
`FUN_14012b5c0(component, null, false)`. That helper queues the current owner for removal. Its
optional clone path still depends on component byte `+0x2d`; the GrowthComponent threshold path
clears that byte before entering this animation route.

Thus the shipped tree descriptions take a two-stage path: request animation 2, then manager callback
after animation completion, then shared cleanup. The exact duration and visible meaning of animation
2 are not assigned here.

## RemoveNaturalResource action

`FUN_1400cffe0` is registered in the `RemoveNaturalResource` Action vtable at slot `+0x20`. It gets
the action's linked component at `+0x28` and verifies that its description is a
`NaturalResourceDescription`. Under a description/state comparison involving description `+0x78`,
the singleton state object and source object `+0x1f0`, it may clear component byte `+0x2d`. It then
sets component byte `+0x2c` from a comparison between two integers on the action's referenced
object, and calls `FUN_14012b5c0` with the action's pointer at `+0x248` and boolean at `+0x30`.

Unlike the growth-transition calls, this path supplies action-owned arguments. Depending on those
arguments and component byte `+0x2d`, the shared helper can update a recipient component, update the
separate resource-category counter, and create/register a raw-material entity as described in
[GAME-LOOP-STEP-15.md](GAME-LOOP-STEP-15.md). Static evidence does not show which system invokes
this Action or when, so this is not yet tied to a specific player command or scheduled interval.

## AnimateNaturalResource action

The `AnimateNaturalResource` Action uses vtable slot `+0x20` for `FUN_1400caad0`. Its serializer
`FUN_1400cab30` stores an internal component reference at `+0x20`, an integer at `+0x28`, and a
boolean at `+0x2c` (in addition to the base Action state). Vtable slot `+0x28` is
`FUN_1400caa80`, which queries whether the linked entity's mesh is currently animating. The method
at slot `+0x20` resolves the referenced component, calls `FUN_14012b910(component, action[+0x28])`,
then returns the stored boolean at `+0x2c`.

This Action therefore enters the same state dispatcher used by the GrowthComponent transition,
but supplies its own integer state and completion result. If the component description's `+0x64`
byte is set, `FUN_14012b910` starts that mesh animation and sets the scheduled bit-`0x04` callback.
If the byte is clear, that dispatcher only takes the immediate shared-cleanup route when flag
`0x200000` was already set. The Action itself does not set that flag and does not directly call
`FUN_14012bc50` or `FUN_14012b5c0`. Its serialized fields establish the dispatch inputs, but do not
reveal which system creates or schedules it.

## Evidence and limits

- `analysis/export/Application-steam-x64.exe/decomp.c`: `FUN_14012bc50` sets flag `0x200000` and
  selects state 2; `FUN_14012b910` selects animation or immediate cleanup; `FUN_14012b110` handles
  animation completion; `FUN_1400c82b0` dispatches the bit-`0x04` set; `FUN_1400cffe0` implements the
  `RemoveNaturalResource` action; `FUN_1400caad0` implements `AnimateNaturalResource`, and
  `FUN_1400caa80` queries its linked mesh animation state; `FUN_14012b5c0` performs shared
  removal/output work.
- Type registration at `0x1401553d0` identifies `NaturalResourceComponent`; vtable slot `+0x40`
  points to `FUN_14012b110`. Action registration and the original PE vtable identify
  `RemoveNaturalResource` and slot `+0x20` -> `FUN_1400cffe0`, plus `AnimateNaturalResource` and
  slot `+0x20` -> `FUN_1400caad0`.
- [ENTITY-COMPONENT-LIFECYCLE.md](ENTITY-COMPONENT-LIFECYCLE.md) documents the manager capability
  sets; [GAME-LOOP-STEP-14.md](GAME-LOOP-STEP-14.md) maps the description flag to shipped resource
  families; [GAME-LOOP-STEP-15.md](GAME-LOOP-STEP-15.md) traces the guarded output path.

## Next trace

Find the Action executor that invokes `RemoveNaturalResource` and `AnimateNaturalResource`, and
identify the values supplied at their serialized fields. Then find producers of
NaturalResourceComponent byte `+0x2d` and consumers of the category counter at `+0x10`; compare
those paths with a decoded save before assigning gameplay names.
