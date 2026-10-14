# Game loop step 17: action sequence dispatch

This step connects the natural-resource Actions from step 16 to their executor and to the
task/controller path that creates them. Evidence is static Ghidra output for the shipped x64 PE,
cross-checked against class registrations and original vtable bytes. It establishes call paths, not
live timing or visible behavior.

## Action sequence dispatcher

`FUN_1400ca120` advances the current entry in an action-sequence stack. The stack index is at
`+0x60`; its entries begin at `+0x48`. For the current sequence, the dispatcher marks each linked
Action active at `+0x18` and invokes its vtable method `+0x20`. A return value of `1` advances to
the next Action in the linked chain. If that phase completes with status `1`, it invokes vtable
method `+0x28` on the chain. If the second phase returns `0`, the dispatcher calls `+0x38` for
Actions marked active, deletes that sequence entry, and backs up the stack index. A status of `2`
routes through `FUN_1400ca8d0`. When the stack index falls below zero, it calls `FUN_1400ca080` on
the separate active chain at `+0x10`; that routine invokes `+0x20` until the chain stops returning
`1`, then clears the chain. The numeric statuses are preserved here without assigning higher-level
meanings not established by the call sites.

## Update sites and cadence limits

`FUN_14010e990` is CharacterComponent's vtable slot `+0x40` and calls `FUN_1400ca120` when the
current sequence index at component `+0x60` is nonnegative. The original vtable bytes at
`0x140168790` and class registration identify this as a CharacterComponent update callback.

A second call site is `FUN_14011b330`, reached in an entity task/job transition path. It invokes the
same dispatcher on the action stack at entity `+0x278 + 0x48`, passing a zero `Time` value. These
sites prove that action advancement is tied to component updates and a task/job transition callback.
The trace does not show a fixed independent action timer or establish the exact global frame order.

CharacterComponent serialization in `FUN_14010ef80` stores an Action array at `+0x48` and a
separate Action pointer at `+0x58`. Both are validated against the registered `Action` base type
after deserialization. This matches the sequence stack and separate active chain used by the
dispatcher; it does not prove that every serialized action is currently runnable.

## Natural-resource Action producers

`FUN_1400c7380` constructs and appends an ordered set of action objects to the stack reached through
the current entity's `+0x278 + 0x48` fields. The sequence includes a `RemoveNaturalResource` Action
from `FUN_1400d02c0`. When the linked resource's byte at `+0x64` is nonzero, it also creates two
`AnimateNaturalResource` Actions through `FUN_1400caca0`, passing `(1, 0)` and `(0, 1)` to their
constructor. The Actions are appended in order; the dispatcher behavior above explains how linked
entries advance, but this evidence alone does not establish their user-visible animation sequence.

`FUN_1400c7380` has four direct callers in resource/job-related routines: `FUN_1400e9660`,
`FUN_1400eb3b0`, `FUN_1400f0390`, and `FUN_1400f3880`. Together with the Action subclass
registrations, this establishes that these classes are instantiated by runtime task/controller code,
not merely present in the type registry. It does not yet map each caller to a named player command.

## Evidence and limits

- `analysis/export/Application-steam-x64.exe/decomp.c`: dispatcher `FUN_1400ca120` at lines
  138268-138346; active-chain processing `FUN_1400ca080` immediately before it; sequence builder
  `FUN_1400c7380` beginning at line 136457; CharacterComponent update `FUN_14010e990` at lines
  186603-186648; serializer `FUN_14010ef80` at lines 186843-186915; task/job transition caller
  `FUN_14011b330` around line 195195.
- The original PE vtable bytes at `0x140168790` map CharacterComponent slot `+0x40` to
  `FUN_14010e990`. Type registrations `FUN_140154510`, `FUN_140152ed0`, and `FUN_140153210`
  identify CharacterComponent and the two natural-resource Actions and their base types.
- `FUN_1400d02c0` is the constructor for the `RemoveNaturalResource` vtable at `0x140166470`;
  `FUN_1400caca0` constructs `AnimateNaturalResource` for vtable `0x140165f20`.
- An extracted-resource CRC search did not identify an AnimateNaturalResource class record. A
  RemoveNaturalResource CRC hit inside a loading texture payload is incidental and is not used as
  class or execution evidence.

## Next trace

Resolve the four `FUN_1400c7380` callers to named job/resource transitions and identify the inputs
that choose the Remove/Animate sequence. Then compare the affected action state with a decoded save
before assigning gameplay meanings to the resource counter and map-cell bytes.
