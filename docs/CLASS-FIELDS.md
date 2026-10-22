# Fields of the main classes

This page explains the field rows in [`labels/fields.tsv`](../labels/fields.tsv) for the citizen,
the building components and the storage. It also lists where the port's structs differ from the
original's fields. Every row is *observed* in the Ghidra export of `Application-steam-x64.exe`,
unless the row says *inferred*. The decompile tools (`tools/triage/decomp.py`,
`tools/gf.sh tools/triage/decomp_query.py`) show the names next to `param_1 + 0x…`.

## Method

1. `tools/triage/types.py class NAME` gives each class's factory, allocation size, constructor and
   vtable. Slot `+0x18` is the serializer.
2. The serializer gives each saved field with its type (`int`, `float`, `bool`, `System::Time`, a
   pointer checked with `IsDerivedFrom`, or a description reference). The save format in
   `crates/formats/src/savegame_schema/` gives the same fields in the same order.
3. The constructor gives the fields that are not saved, and the starting values.
4. `decomp_query.py layout` over the class's methods gives the remaining offsets and their sizes.
   The Rust structs that port each class, and the functions they cite, give names.
5. A check script read each row and confirmed that each cited function uses that offset. The
   cases it could not confirm by text are an indexed loop (the AI cooldowns), a write that covers
   several fields at once (the state machine's four bytes), and an access through another pointer
   (the citizen description through `+0x288`). Each of them was read by hand.

## The classes

| Class | Size | Factory | Constructor | Serializer |
|---|---:|---|---|---|
| CitizenComponent | `0x290` | `FUN_140030a70` | `FUN_140119c40` | `FUN_140119630` (version 3) |
| CitizenAIComponent | `0xd0` | `FUN_140030480` | `FUN_140115040` | `FUN_140114e30` (version 5) |
| CharacterComponent | `0x80` | `FUN_1400302c0` | `FUN_14010f1f0` | `FUN_14010ef80` |
| StorageComponent | `0x40` | `FUN_1400394b0` | `FUN_140131da0` | `FUN_140131c00` (version 1) |
| WorkPlaceComponent | `0x68` | `FUN_14003a990` | `FUN_1401347b0` | `FUN_1401345f0` (version 1) |
| ResidenceComponent | `0x60` | `FUN_140037da0` | `FUN_14012f770` | `FUN_14012f640` |
| BuildComponent | `0x78` | `FUN_14002fc10` | `FUN_14010e490` | `FUN_14010e250` (version 1) |
| WorkComponent | `0x38` | `FUN_14003a7d0` | `FUN_140133cf0` | `FUN_140133be0` (version 3) |
| StateMachineComponent | `0x58` | `FUN_140038c30` | `FUN_140130f00` | `FUN_140130e10` |
| DestroyComponent | `0x68` | `FUN_140034520` | `FUN_140123150` | `FUN_140123020` |
| ClearAreaComponent | `0x50` | `FUN_140031800` | `FUN_14011d470` | `FUN_14011d390` |

A building in the original is an entity with several of these components: a StateMachineComponent,
a BuildComponent while it is built, and a StorageComponent, WorkPlaceComponent,
ResidenceComponent, WorkComponent, DestroyComponent and ClearAreaComponent as its template asks.
There is no single Building class. The Entity rows come from
[ENTITY-COMPONENT-LIFECYCLE.md](ENTITY-COMPONENT-LIFECYCLE.md).

## Shared layouts

- **Component** (`+0x00..+0x27`): the vtable, the internal object header, the owning entity
  (`+0x10`), the description (`+0x18`) and the capability bits (`+0x20`).
  `Component::Serialize` (`FUN_14011de80`) saves the first three. `decomp.py` does not follow
  inheritance, so every component class repeats these rows.
- **ControllerComponent** (`+0x28..+0x47`): an embedded controller interface. `+0x28` is its
  vtable, `+0x30` and `+0x38` link it into the state machine's chain, and `+0x40` points back at
  the component. `StateMachineComponent::Setup` (`FUN_140130b70`) walks the entity's components in
  order and links each controller's interface after the previous one. The interface comes from
  slot `+0x88`. The first one also gives the starting state, from slot `+0x80`.
- **StateMachineComponent** holds the head of that chain at `+0x28` and the current controller at
  `+0x48`. The four bytes at `+0x50` are the state, its index, the pending state and the pending
  transition. The constructor writes them as one value, `0xff00ffff`. `StateMachine::Run`
  (`FUN_1400aeb30`) takes a pointer to `+0x48`, so in that function they are `+0x8..+0xb`.
- **Arrays** are a pointer and an `int` count in the next field. The storage also keeps the
  allocated size at `+0x34` (*inferred*: `Storage::add` and `Storage::reserve` write it when the
  array grows). A storage item is 16 bytes: material (`+0x0`), count (`+0x8`), reserved (`+0xc`).
  A residence's family is 16 bytes: a citizen array and its count.

## Differences between the port and the original

| Class | Field | Original | Port | Effect |
|---|---|---|---|---|
| CitizenComponent | `+0x240` family slot | The constructor does not write it | `newborn` sets -1 | Open question below |
| CitizenComponent | `+0x28` name, `+0x40` lifespan | Saved as a string and a float. `People::update` (`FUN_140118340`) reads the lifespan | `Citizen::name` and `Citizen::lifespan` say "Not identified yet" | Doc comments only |
| BuildComponent | `+0x50` | Saved, and set to 0 by the constructor | Not ported (the save schema reads it as `flag_50`) | None known. No reader was found |
| StorageComponent | `+0x38`, `+0x3c` | Capacity, then accepted flags (the activate slot `FUN_140131b10` writes both) | `Storage` agrees | The comment on the schema entry in `savegame_schema/` puts "capacity" at `+0x3c`. Comment only |
| StateMachineComponent | `+0x52` pending | The constructor sets 0. Setup sets the first controller's starting state | `Machine::new` sets it from the first controller | None. The comment on `Machine` writes `+10` for `+0xa` |

The CitizenComponent constructor sets the tool status (`+0xa8`) to -1 and the clothing status
(`+0x1e0`) to 4. `CitizenState::newborn` and the starting citizens in `Agents::new` used 0 for
both, and now use the constructor's values. The achievements count both values: a working citizen
with a tool status below 0 counts for the tool goal, and clothing status 4 counts for both kinds of
coat. `tools/compat/compare.sh` against the build before the change found only these two fields
different, in citizens that had no tool or coat yet.

The earlier rows named `+0x1e0` "clothing" and `+0x1e4` "insulation". The serializer saves
`+0x1e4` first, then the time at `+0x1e8`, then `+0x1e0` from version 2. Message 6 and 7 in
`CitizenState::message` (`FUN_140117730`) set both to the coat level. The rows now follow the
port: `+0x1e0` is the status shown in the citizen window, and `+0x1e4` is the level that keeps the
citizen warm.

The other fields agree with the port: `WorkPlace` (profession `+0x48`, enabled `+0x50`, workers
`+0x58`), `Home` (fuel `+0x48`, families `+0x50`), `Destroy` (`+0x48..+0x60`), `Site` (progress
and stages `+0x48`, `+0x4c`, work id and kept priority `+0x28`, `+0x2c`, clear total `+0x48`),
`Storage`, `CitizenState` and `CitizenAi`.

## Fields found that the port does not keep

- CitizenComponent `+0xb8`: the tool's mesh instance, attached to the citizen's model. Show tool
  (`FUN_14011c880`) fills it. Hide tool (`FUN_14011cbb0`) detaches it and removes the scene object
  whose handle is at `+0x1dc`. `+0x1d8` is the shown tool, which the port keeps as
  `CitizenState::shown_tool`.
- CitizenComponent `+0x278`, `+0x280`, `+0x288`: the character, the status icon and the
  description, which the setup slot (`FUN_140119520`) looks up. The port reaches them by index.

## Open questions

- Is the CitizenComponent's memory zeroed before the constructor runs? If it is, the family slot
  `+0x240` starts at 0 in the original. The factory (`FUN_140030a70`) and the object allocator
  were not traced.
- `+0x84` of the CitizenComponent: the happiness step (`FUN_140117fb0`) compares it with
  `_workForHappiness`. No store to it was found in the Application, which the port already notes.
- StateMachineComponent `+0x48` starts at the machine's own interface, the head of the chain. The
  port starts `Machine::current` at the last controller. The run loop searches back from the
  current controller, so the two may be the same in effect. This was not checked.
