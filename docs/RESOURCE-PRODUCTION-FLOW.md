# Workplace production and goods transfer

This is a bounded static trace of the Steam x64 `Application-steam-x64.exe`, SHA-256
`323e0a12d1ef36605db48d44210745e7043240f07d2d0ae2cf7e13e3c727a5ed`, in the existing
Ghidra 12.1.4 `analysis/gameplay-project/banished-gameplay` project. Addresses are image virtual
addresses. Decompiled output establishes code operations and call relationships; no original-game
execution or tests were performed for this note. Exact recipe yields and the complete scheduling
policy remain open.

## What the trace establishes

Workplace components construct work objects that schedule actions. Storage uses a common ledger
for buildings and citizens. A stored-goods pickup reserves stock, subtracts the quantity from its
source, and adds it to the citizen's ledger. Delivery checks destination compatibility and capacity
before adding to the destination and subtracting from the citizen. The market's work callback
uses this same task-building and reservation path.

The recipe work callback and action construction are traced below. The callback which actually
creates each recipe's output quantity was not followed through to its final inventory mutation;
therefore the complete production-to-delivery chain is only partially established.

## Class identity and component handoff

These names come from `Object::BaseTypeInfo::BaseTypeInfo` calls, rather than guesses from addresses.

| Registration | Proven type relationship / factory |
| --- | --- |
| `0x140155750` | `ResourceProduceComponent` derives from `ControllerComponent`; its factory `0x140003730` returns null. |
| `0x140154950` | `ConsumeProduceComponent` derives from `ResourceProduceComponent`; factory `0x140032c60` allocates 0x70 bytes and calls constructor `0x14011e520`. |
| `0x140150f40` | `MarketComponent` derives from `ResourceProduceComponent`; factory `0x140008220` calls the shared constructor `0x1401308a0` and installs vtable `0x14015fd60`. |
| `0x140155990` | `StorageComponent` derives from `Component`; factory `0x1400394b0` calls constructor `0x140131da0`, which installs vtable `0x14016ab50`. |
| `0x140155610` | `RawMaterialDescription` derives from `Description`; its runtime type record is `DAT_1401cfcb8`. |
| `0x140154990` | `ConsumeProduceDescription` derives from `Description`; type record `DAT_1401cf388`. |
| `0x140150f80` | `MarketDescription` derives from `Description`; type record `DAT_1401cc528`. |
| `0x140153bd0`, `0x140153e50` | `ConsumeProduce` and `MarketWork` are registered as `Work` subclasses. |

`ConsumeProduceComponent` vtable `0x140168eb0`, slot 18, points to `0x14011e1d0`. This
function finds the owning template's `ConsumeProduceDescription`, allocates a 0x78-byte work
object using `0x1400ee6d0`, and passes description fields `+0x38` and `+0x3c`. That constructor
installs work vtable `0x140167640`; slot 4 points to the scheduling callback `0x1400ecd10`.

`MarketComponent` slot 18 points to `0x14004f3a0`: it allocates a 0x58-byte work object,
calls shared work constructor `0x1400fc7b0`, and installs vtable `0x140167b40`. Its slot 4
points to `0x1400f5450`. These component-to-work-to-callback edges are direct code evidence.
The meaning of every virtual slot and the complete caller/update order are not recovered here.

## Common inventory representation

`0x140006270(owner, result)` walks an internal object's component array and returns its
`StorageComponent`, testing `DAT_1401cfe78` and `Object::Base::IsDerivedFrom`.

| Storage offset | Meaning established by reads and writes |
| --- | --- |
| `+0x28` | Pointer to an array of 16-byte resource entries. |
| `+0x30`, `+0x34` | Entry count and allocated entry capacity. |
| `+0x38` | Maximum weighted storage occupancy. |
| `+0x3c` | Accepted resource-category bitmask. |
| Entry `+0x00` | Resource-template pointer. |
| Entry `+0x08` | Signed 32-bit stored quantity. |
| Entry `+0x0c` | Signed 32-bit reserved quantity, supported by the matching reservation and pickup mutations below. |

`0x1401323e0` computes occupancy as the sum of each stored quantity multiplied by its
`RawMaterialDescription +0x48` integer. The operational meaning of `+0x48` is therefore the
resource's storage weight/size per unit. `0x140131a30` accepts a resource only when the
description's `+0x5c` flags intersect the storage `+0x3c` mask. The description serializer
`0x14012e9f0` independently confirms that `+0x48` is an integer and `+0x5c` is a serialized
flag-like field; it does not provide their original field names.

| Routine | Observed operation |
| --- | --- |
| `0x140132250(storage, resource, quantity)` | Tests the category mask, then requires `currentOccupancy + unitWeight * quantity <= storageCapacity`. |
| `0x1401324d0(storage, resource, quantity)` | Adds to an existing quantity or appends an entry containing template, quantity and a zero reserved field. It does **not** perform its own capacity check. |
| `0x140132620(storage, resource, quantity)` | Subtracts from the matching quantity and removes the entry when the result equals zero. The examined routine does not clamp negative results. |
| `0x14011bc90(citizenAI, resource)` | Obtains the owner's storage and returns `stored - reserved`, or zero if absent. |
| `0x14011bc30(citizenAI, resource, quantity)` | Obtains the owner's storage and calls the subtraction routine. |
| `0x14011ba70(citizenAI, resource, quantity)` | Obtains the owner's storage and calls the addition routine. |

This extends the capacity-aware starting-inventory trace in
[STARTING-INVENTORY-DISTRIBUTION.md](STARTING-INVENTORY-DISTRIBUTION.md): its destination type
and raw-material type are now identified. The names of all individual category bits remain open.

`0x140131b10` also shows initialization from `StorageDescription`: when its `+0x21` boolean is
set, it clears the ledger, sets capacity from description `+0x24`, copies the mask from `+0x28`,
and adds the initial resource/quantity entries from `+0x30`/`+0x38`. If description `+0x20` is
set, capacity is multiplied by two owner footprint dimensions returned through `0x140004680`.
The original boolean names and footprint coordinate conventions are not established here.

## Recipe selection and action construction

`0x14011e5b0` serializes `ConsumeProduceDescription`'s array at `+0x28` with count at `+0x30`.
The scheduling callback `0x1400ecd10` indexes its records with a 0x28-byte stride. These are
recipe-like alternatives; the term “recipe” is a descriptive interpretation supported by their
use as resource/quantity lists, not a recovered C++ field name.

`0x14011e3c0` initializes the component's active selector at `+0x50` from description `+0x4c`,
sizes the component's float array at `+0x58`/`+0x60` to the description's alternative count,
and fills that array with 1.0. `0x14011e940` returns a nonnegative selector directly. For selector
`-1`, it sums those float weights, draws `RandFloat`, and chooses the first cumulative interval
containing the draw. Other negative modes were not fully traced.

The work callback `0x1400ecd10`:

1. Resolves the owning `ConsumeProduceComponent` and `ConsumeProduceDescription`, and calls
   `0x14011e940` to obtain the selected alternative.
2. Consults a manager table at `manager +0x170`, indexed by the alternative's record `+0x18`,
   comparing two integers at table-entry `+0` and `+8`. This is a production-admission/limit
   check by use; the counter names and complete category meanings remain unconfirmed.
3. Checks work-slot and storage state. Some branches call `0x14011b330` to arrange disposal
   of goods already carried by the citizen; others call the generic transfer builder
   `0x1400c4c60` to move resources between workplace and citizen.
4. When admitting a production operation, calls `0x1400c4440` with the chosen alternative,
   owner/template references, work position and description `+0x44` duration-like float.

`0x1400c4440` clears the citizen's action collection at the object reached through
`citizenAI +0x278`, collection offset `+0x48`, and appends actions through `0x1400ca9b0`.
In the production-enabled branch it constructs `0x1400cc670` using the selected record's
`+0x10` pointer, appends timed work through `0x1400cebf0`, then constructs `0x1400cb910` and
adds resource/quantity pairs from the selected record's `+0` array, count `+8`, using
`0x1400cb980`. Movement actions are constructed with `0x1400ce640`.

The registrations prove that `ConsumeRawMaterial` (`0x140152f90`), `CreateRawMaterial`
(`0x140153010`), `DropOffRawMaterial` (`0x140153050`), `PickupRawMaterial` (`0x140153150`)
and `PickupStoredRawMaterial` (`0x140153190`) are distinct `Action` classes. The named
factory-to-constructor-to-execute link for each production action was not completed. In
particular, this note does not identify the last resource list above as inputs or outputs solely
from its position in the action collection, and does not assume collection insertion order is
execution order. Recipe yield, education/tool modifiers, and exactly where newly produced goods
first appear remain explicit gaps.

## Reservations, pickup and delivery

The transfer builder `0x1400c4c60` installs a sequence of action objects into the same citizen
collection. It accepts two owner references and positions, constructs movement/timed actions,
constructs the pickup object with `0x1400cf9f0`, returns a reference to that pickup object, and
appends it through `0x1400ca9b0`. The complete ordering semantics of the collection remain open.

`0x1400cf9f0` obtains storage from its source owner at object `+0x28`. If a resource is supplied,
it computes a pickup count from the requested count or the citizen's carrying allowance divided
by unit weight, caps it at `stored - reserved`, and calls `0x1400cf700`.

`0x1400cf700` increments that resource entry's reserved field at `+0x0c`, then adds or merges the
resource/count pair into the pickup object's own list at `+0x30`, count `+0x38`. This is direct
evidence for interpreting the ledger's second integer as a reservation rather than another stock
quantity.

Execution callback `0x1400cf210` visits that pickup list. For each resource it checks the available
source quantity; its object `+0x40` boolean selects whether a shortage fails or permits a partial
count. It decrements the reserved field by the actual count, clamping the reservation to zero,
calls `0x140132620` on the source storage, resolves the citizen owner's storage via the reference
at action `+0x20`, and calls `0x1401324d0` with the same resource and count. If the requested count
is nonpositive, another branch derives a count from the citizen storage's remaining weighted
capacity divided by resource unit weight.

Delivery helper `0x1400ccaf0` is called by `0x1400cc7d0`, including call sites
`0x1400cc843` and `0x1400cc9db`. It obtains the carried resource's available amount through
`0x14011bc90`, applies its action's `+0x24` maximum when nonnegative, and calls
`0x140132250` on the supplied destination storage. On success it calls `0x1401324d0` on that
destination and `0x14011bc30` on the citizen for the same count. This preserves quantities across
the two ledgers in the successful path.

If delivery cannot complete and action `+0x20` is set, the helper searches for another destination
through `0x140092a60`; if none is returned, it calls `0x1400ccd90` and returns a different status.
That fallback's exact effects and all status-enum meanings were not traced.

`0x140092a60` has two search modes: one compares unreserved stock against a requested amount;
the other calls the storage capacity predicate. It filters by resource flags and evaluates a
map/path-related cost through `0x1400a4ce0`. It retains a candidate with a lower cost. “Nearest
usable storage” is a reasonable interpretation, but exact distance metric, tie behavior, search
coverage and accessibility policy require further analysis.

## Markets reuse the same inventory and transfer machinery

`0x1400f5450` resolves its workplace `MarketComponent`, `MarketDescription` and market
`StorageComponent`. If the citizen already carries a positive weighted amount, it calls
`0x14011b330` and returns before selecting another collection task.

It compares `marketOccupancy / marketCapacity` with `MarketDescription +0x20` and returns
without scheduling a new collection when the fraction reaches that threshold. Thus the float's
operational meaning is a market fill-fraction threshold. Its exact original field name and all
configuration values remain unconfirmed. Serializer `0x14004f510` confirms the float followed
by an array of 8-byte records containing a flag-like value and another float; this trace does not
fully recover that array's prioritization formula.

For selected resources, the callback bounds each planned count using the citizen carrying budget
divided by resource weight, a candidate quantity, and a remaining category budget divided by
weight. When it has a nonempty plan, it calls `0x1400c4c60`, then calls `0x1400cf700` on the
returned pickup object for each planned resource/count pair and records the citizen and plan in
the work object's map at `+0x48`.

This proves that market restocking schedules reserved pickups and uses the common transfer path.
It does not prove that every produced good is sent to a market, that market vendors are the only
haulers, or which storage is selected first for each category. `0x14011b330` independently searches
destinations for carried goods using `0x140092a60` and schedules resulting work, so carrying and
storage allocation also exist outside the market-specific selection callback.

## Evidence retained locally and remaining scope

Focused exports from this pass are in `/tmp/resource-factories.txt`,
`/tmp/resource-constructors.txt`, `/tmp/resource-vtables.txt`,
`/tmp/resource-workplace-methods.txt`, `/tmp/resource-ai-constructors.txt`,
`/tmp/resource-controller-methods.txt`, `/tmp/production-core.txt`,
`/tmp/production-execution.txt`, `/tmp/resource-action-registrations.txt`, and
`/tmp/resource-actions.txt`. Cross-reference exports are `/tmp/resource-refs.txt`,
`/tmp/resource-handoff-refs.txt` and `/tmp/production-class-refs.txt`. These are temporary local
evidence, not durable repository artifacts; the addresses above allow repeat export with tracked
`tools/ghidra/ExportAddressBatch.java` and `ExportVtableSlots.java`.

The storage schema, reservation mutation and successful pickup/delivery quantity operations are
established. Work construction and recipe/market scheduling are partially established. Output
creation, complete input consumption, exact recipe values, all worker modifiers, reservation
cancellation, action execution order, full destination selection and original-game runtime
confirmation remain unresolved. No gameplay implementation or tests were added by this trace.
