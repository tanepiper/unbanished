# Entity construction and component lifecycle

The game builds a `ComponentEntity` from a cached `ComponentDescription` resource. That resource
contains an ordered list of external `Description` objects. Each description supplies a runtime
component type ID or zero. Construction allocates all components, assigns their owner and
description pointers, and then initializes them. A separate manager registration step activates
them. Updates and teardown use capability bits and virtual-method slots.

## Evidence and scope

This is a static trace from the existing **Ghidra 12.1.4** exports in
`analysis/export/Application-steam-x64.exe/decomp.c` and
`analysis/export/Runtime-steam-x64.dll/decomp.c`. Addresses below are image virtual addresses.
The analysis used the exported pseudocode and function inventory, plus read-only PE vtable bytes
to connect the observed model-component methods to their slots. No target execution or tests
were performed for this investigation.

| Artifact | SHA-256 |
|---|---|
| `Application-steam-x64.exe` | `323e0a12d1ef36605db48d44210745e7043240f07d2d0ae2cf7e13e3c727a5ed` |
| `Runtime-steam-x64.dll` | `d1c777b56674303ee276e807b88a7b73089d779f5dc923731ed45ef214cc7936` |

Named engine methods below retain their exported identities. Application methods remain
`FUN_<address>` in Ghidra; descriptive phase names are interpretations of their observed calls,
not recovered C++ method names. Decompiled argument lists can omit register arguments, so slot
offsets, explicit field writes and direct-call relationships are stronger evidence than those
reconstructed signatures.

## Resource loading and template identity

`Object::ExternalFactory::FindOrLoad` (`Runtime:0x1800c1b90`) expands relative paths and checks its
resource cache. On a miss it constructs the compiled resource path, invokes `Object::External::Load`
(`0x1800bee90`), records the requested resource path on the loaded external object and adds it to
the factory cache.

`External::Load` opens a compressed file stream, reads resource magic `0x5412a91f`, reads the type
ID, allocates the registered type, verifies its `Object::External` ancestry, checks the resource
version, reads dependencies and calls the external object's virtual serializer at slot `+0x28`.
The factory's ordinary allocating overload, `Object::Factory::CreateObject` (`0x1800c3350`), looks
up the type ID and invokes its registered allocation callback; the overload at `0x1800c3310`
instead invokes the in-place callback.

Application type registration proves the relevant identities:

| Registration function | Registered class | Parent | Allocating callback |
|---|---|---|---|
| `0x140152e50` | `ComponentEntity` | `Object::Internal` | `0x140022320` |
| `0x140154890` | `Component` | `Object::Internal` | abstract/base callback |
| `0x1401548d0` | `Description` | `Object::External` | abstract/base callback |
| `0x140154910` | `ComponentDescription` | `Object::External` | `0x140032890` |
| `0x140155250` | `ModelComponent` | `Component` | `0x140036230` |
| `0x140155290` | `ModelDescription` | `Description` | `0x140036290` |

`ComponentDescription` is the entity template container, despite its singular name. Its serializer
(`0x14011dfc0`) writes a u32 at object offset `+0x20`, a count, and the external description pointers
stored at `+0x28` with count `+0x30`. The pointer-array serializer (`0x1400329f0`) resolves paths
through `FindOrLoad` and checks `Description` ancestry. The constructor (`0x140032890`) defaults
the u32 to 6. Manager registration below establishes that this value selects a scheduling bucket.

Reading those fields from the player's extracted resources corroborates this layout:

| Template | Bucket | Ordered description references |
|---|---:|---|
| `Template/Tornado.rsc:resource` | 1 | `particle`, `ambientemitter`, `tornado` |
| `Template/TraderBoat.rsc:resource` | 1 | `model`, `character`, `particle`, `statemachine`, `boat` |
| `Template/Citizen.rsc:resource` | 1 | `ui`, `toolbar`, `model`, `picking`, `highlight`, `character`, `statusicon`, `storage`, `citizen`, `citizenai` |

There are 155 shipped `ComponentDescription` resources. Their bucket counts are 1:4, 2:2, 3:91,
4:29 and 6:29. These are observed data values; the original category names and intended cadence
have not been established.

## Construction and attachment

`FUN_140060680` is a concrete path from a resource name to a live entity:

1. Construct the filename `Template/Tornado.rsc` and call `FindOrLoad`.
2. Require the result to derive from `ComponentDescription`.
3. Allocate `0x48` bytes and call `FUN_1400c8bf0` with that template.
4. Wrap the result in an `Object::InternalBasePtr`.
5. Register it with the manager at global-context offset `+0xe8`, using the template's `+0x20`
   bucket value, through `FUN_1400c9d70`.

`FUN_1400c8bf0` is the template-taking entity constructor. It performs two passes over the
template's description array. First it asks each description's virtual slot `+0x40` for a u32;
only nonzero results contribute to the runtime component count. It allocates the component pointer
array once. The second pass repeats that query, calls `Factory::CreateObject` for each nonzero
type ID, checks `Component` ancestry, and writes both attachment fields:

| Runtime object | Offset | Established meaning |
|---|---:|---|
| Entity | `+0x10` | Manager pointer, initially null |
| Entity | `+0x18` | Index in manager's main entity array, initially -1 |
| Entity | `+0x1c` low 24 bits | Index within its bucket, initially `0xffffff` |
| Entity | `+0x1f` | Bucket byte |
| Entity | `+0x20` | Last scheduled-update timestamp |
| Entity | `+0x28` | External `ComponentDescription` pointer |
| Entity | `+0x30` / `+0x38` | Runtime component-pointer array / count |
| Entity | `+0x40` | Aggregate component capability bits plus lifecycle state |
| Component | `+0x10` | Owning entity as `InternalBasePtr` |
| Component | `+0x18` | External description pointer |
| Component | `+0x20` | Capability bits |

All allocated components have their owner and description assigned **before** the constructor
runs its initialization pass. `FUN_1400c8940` ORs the low 16 component capability bits into the entity's
aggregate mask. `FUN_1400c8570` then invokes component slot `+0x20` for components carrying bit
`0x80`. This ordering permits initialization to look up other attached components. That lookup
is visible in `FUN_1400c8b50`: scan the entity array, compare each component's virtual type-ID
method at `+0x10`, return the matching internal pointer or null.

Manager registration, `FUN_1400c9d70`, assigns manager/index/timestamp fields, queues capability-set
membership updates, appends the entity to the main array, and appends it to the selected bucket
unless the bucket is 6. It then calls `FUN_1400c8430`, which invokes component slot `+0x30` for
components carrying bit `0x01`. If the manager's byte at `+0xe0` is set, registration deletes the
incoming internal pointer instead; manager clearing sets that byte around bulk deletion.

## Lifecycle phases and the model-component example

The phase labels below describe the traced behavior. Slots are on the **runtime component**
vtable, not on description or entity vtables.

| Entity dispatcher | Required component bit | Component slot | Observed behavior |
|---|---:|---:|---|
| `0x1400c8570` | `0x80` | `+0x20` | Initial setup after all components attach |
| `0x1400c84d0` | `0x100` | `+0x28` | Pre-destruction callback, called by entity destructor |
| `0x1400c8430` | `0x01` | `+0x30` | Activation after manager registration |
| `0x1400c8370` | `0x02` | `+0x38` | Removal callback, followed by entity teardown bit `0x400` |
| `0x1400c82b0` | `0x04` | `+0x40` | Update using the manager's current delta argument |
| `0x1400c81f0` and bucket loop below | `0x08` | `+0x48` | Update using an elapsed-time argument |
| `0x1400c8140` | `0x10` | `+0x50` | Additional update phase, no explicit time parameter recovered |

The model component makes the first, third, fourth and fifth phase meanings concrete. Its
constructor `FUN_14012a1f0` creates an embedded `Graphics::MeshInstance` at `+0x28`, starts with
capability bits `0x83` and initializes its scene handle at `+0x16c` to -1. Raw vtable entries at
`0x140169fd0` corroborate these Ghidra-exported targets:

- Slot `+0x20` → `FUN_140129f90`: read its `ModelDescription`, choose a variant, call
  `Graphics::MeshInstance::SetMesh`.
- Slot `+0x30` → `FUN_140129f00`: apply transform, update the embedded mesh at time zero, call
  `Graphics::Scene::AddObject`, retain the scene handle.
- Slot `+0x38` → `FUN_140129ea0`: call `Graphics::Scene::RemoveObject`, reset that handle to -1.
- Slot `+0x40` → `FUN_140129e30`: advance the embedded mesh; clear capability bit `0x04` when
  `MeshInstance::IsAnimating` becomes false and recompute the owner's aggregate mask.

`FUN_14011de20` is a generic component bit setter: `(old | set_bits) & ~clear_bits`. A change to
the low 16 bits triggers owner-mask recomputation. `FUN_1400c8940` queues membership changes for
the manager's bit-`0x04` and bit-`0x10` entity sets when either aggregate capability changes.

## Scheduling, deletion and loading existing entities

`FUN_1400c9560` advances the manager's clock, processes queued bit-`0x04` membership changes and
updates that ordered entity set through `FUN_1400c82b0`. It then visits **five** bucket arrays,
using per-bucket cursor and quota values. For each visited live entity it calculates
`manager_time - entity_last_time`, dispatches slot `+0x48` to bit-`0x08` components when positive,
and records the new last time. Bucket 6 is explicitly excluded at registration; no shipped
template in this data selects 0 or 5.

`FUN_1400c98a0` separately processes the bit-`0x10` set through `FUN_1400c8140`. The application
update function `FUN_14009b850` calls the timed manager update inside its simulation-step loop
after `Graphics::AnimationManager::Update`, and calls `FUN_1400c98a0` once after that loop. The
GrowthComponent trace in [GAME-LOOP-STEP-11.md](GAME-LOOP-STEP-11.md) confirms this call relationship:
the timed update repeats with the simulation iteration count, while the bit-`0x10` pass still runs
once when simulation speed is paused. A UI state that pauses the game bypasses both passes. Bucket
quotas, exact scheduled intervals and original phase names remain unresolved.

`FUN_1400c9ea0` requests removal: if the entity exists and is not already marked, it invokes
`FUN_1400c8370` and appends the pointer to the manager's pending-removal array at `+0xd0` with
count `+0xd8`. The timed manager update drains that array. `FUN_1400c8db0` swap-removes the entity
from its bucket and main array, repairs the moved entities' indices, removes it from the two
capability sets, then calls `InternalBasePtr::DeletePtr`.

The entity destructor, `FUN_1400c8a20`, first runs the `0x100` callback pass, performs another
global cleanup call whose semantics remain unassigned, clears each component pointer slot and
calls `DeletePtr` on the prior component pointer, frees the component array, then destroys the
`Object::Internal` base. `FUN_1400c9200` bulk-clears the manager using the same removal dispatch
and deletion helper.

Loading existing entity state is distinct from new construction. `FUN_1400c8610` reads the template
reference, component pointers and aggregate mask. On the read path it compares the current
template's expected nonzero component IDs with the restored array. A missing or mismatched entry
causes allocation of the expected type, owner/description attachment, direct slot-`+0x20`
initialization, and insertion at the current template position. It finally recomputes aggregate
capabilities. This is evidence of reconciliation against the current template, **not** proof of
general compatibility with every template edit; removal/reordering cases were not exhaustively
traced. The base component persistence routine `FUN_14011de80` separately serializes its owner
pointer, capability mask and description reference.

## Remaining unknowns

- Original C++ phase names and bucket names. Quotas and cursor/elapsed-time behavior are now
  recovered and emulator-verified in [GAME-LOOP-STEP-18.md](GAME-LOOP-STEP-18.md).
- The complete description-type-to-component-type mapping, including every description that
  returns zero from slot `+0x40`.
- All message/event slots after those listed above, component-specific state machines and
  cross-component dependency rules.
- The entity destructor's global cleanup helper and every invalidation/failure path of
  `InternalBasePtr`.
- Exact save migration guarantees when descriptions are removed or reordered.

For a reimplementation, the evidence supports keeping external descriptions cached and runtime
components per entity, allocating and attaching the whole component array before setup, treating
activation as a separate phase, preserving phase order, and deferring destruction until manager
iteration finishes. Those are design implications of the static trace, not claims that the
repository already implements the simulation runtime.
