# Game loop step 09: citizen status flag producers

This step follows the `StatusIconComponent` flag word backward from the Status Bar consumer into
citizen update code. Evidence is static Ghidra decompilation of the Steam x64 executable already
exported in `analysis/export/Application-steam-x64.exe/decomp.c`. It is not a live runtime trace.

## Flag storage and setter

`StatusIconComponent` is registered by `FUN_140155910` (`0x140155910`) as a `Component`. Its
constructor `FUN_140131590` (`0x140131590`) zeroes the 32-bit flag word at `+0xF0`. Its serializer
`FUN_1401314c0` (`0x1401314c0`) writes and restores that word, so the status flags are preserved with
the component state.

`FUN_1401318c0` (`0x1401318c0`) changes a selected bit: it forms `1 << bit`, clears the bit when its
boolean argument is false, and sets it when true. It then adds or removes the component's geometry
from the graphics scene: a zero word removes the object, while a nonzero word can add it when the
global scene switch is enabled. It also invokes the component's vtable callback at `+0x40` with a
zero time value. The flags therefore drive both the Status Bar's counts and a world graphics
component; the exact visual role of that geometry is still open.

## Per-update predicates

The citizen-associated update routine `FUN_140118340` (`0x140118340`) finds the citizen's
`StatusIconComponent` and refreshes bits 0–3 and 5 with explicit boolean values:

| Bit | Predicate passed to the setter | Status Bar meaning |
|---:|---|---|
| 0 | Citizen pointer at `+0x238` is null | No home |
| 1 | Midpoint of description floats `+0xCC..+0xD0` is less than the citizen value at `+0x44` | Hungry |
| 2 | Threshold 25% from description float `+0xEC` toward `+0xF0` is less than the citizen value at `+0x48` | Freezing |
| 3 | Byte at `*(citizen + 0x1F0) + 0x4C` is clear and pointer at `+0x248` is null | No job |
| 5 | Citizen pointer at `+0x68` is non-null | Has disease |

The predicate expressions are observed in the decompilation. The user-facing meanings come from
the matching `StatusHomeTip`, `StatusFoodTip`, `StatusColdTip`, `StatusJobTip`, and
`StatusDiseaseTip` strings and their paired Status Bar groups. Field names beyond those established
by the resource/code correspondence remain unknown.

Bit 4 is touched through a separate callback path: `FUN_140117730` (`0x140117730`) dispatches event
case 1 to `FUN_1401152b0` (`0x1401152b0`), which calls `FUN_1401318c0` for bit 4 and updates two
integer fields on the citizen. The Status Bar and per-citizen tooltip identify bit 4 as the tool
condition (no usable tool / broken tool). The decompiled call omits the setter's third argument, so
this export alone does not establish whether that callback sets or clears the bit at that point;
other lifecycle code may complete the transition.

## Loop connection and remaining work

`Game/Game::Update` runs the entity manager for each active simulation iteration. The entity
manager's scheduled component update is the broader cadence path recorded in
[step 02](GAME-LOOP-STEP-02.md) and [ENTITY-COMPONENT-LIFECYCLE.md](ENTITY-COMPONENT-LIFECYCLE.md).
This step identifies a citizen update path that refreshes status flags within that simulation
system; no timing capture was made.

Still open: trace all tool-status set/clear transitions, identify the exact citizen component type
that owns the `+0x238`, `+0x248`, and `+0x68` state, and identify the visual geometry controlled by
`StatusIconComponent`. The game port still has no citizen entities or status simulation.

## Evidence pointers

- `FUN_140155910` (`0x140155910`): `StatusIconComponent` type registration.
- `FUN_140131590` (`0x140131590`): construction and initial flag word.
- `FUN_1401314c0` (`0x1401314c0`): flag word serialization at `+0xF0`.
- `FUN_1401318c0` (`0x1401318c0`): bit mutation and geometry activation.
- `FUN_140118340` (`0x140118340`): citizen-associated update predicates for bits 0–3 and 5.
- `FUN_140117730` / `FUN_1401152b0`: tool-status event dispatch path for bit 4.
- Local Ghidra export: `analysis/export/Application-steam-x64.exe/decomp.c`.
