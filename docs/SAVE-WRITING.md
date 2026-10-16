# Saving a game

The port saves a town by writing its state back into the game state it was loaded from. The
code is in `apps/game/src/start/save.rs` and its submodules. The writer then encodes the result
(`GameState::encode`) and builds the `.sav` file (`Save::write`); see
[SAVE-GAME-STATE.md](SAVE-GAME-STATE.md) and [SAVE-FORMAT-AND-RESTORE.md](SAVE-FORMAT-AND-RESTORE.md).

Every field the load reads ([SAVE-LOADING.md](SAVE-LOADING.md)) is written back by the same
offset. A loaded town remembers the saved object each entity and citizen came from
(`StartingTown::save_ids`).

A value the load derives is rewritten only when the port's value differs, so an untouched
object keeps its exact bits. A model's rotation is one: saves show `(0, 0, −sin(yaw/2),
cos(yaw/2))`, and it is recomputed only when the yaw changes.

## Checking it against the original

The check is `start::save::tests::vanilla_saves_export_unchanged`. Each vanilla save is loaded
and written straight back. The encoded state must equal the original byte for byte, except for
the floor counts under active floors: the original's FloorComponent load slot (`FUN_140124a10`)
adds one there, so a save written straight after a load holds one more. A wrongly inverted field
changes the bytes. The test then lists the differing objects and fields.

## Written so far

- **Stage 1:**
  - each entity's map rectangle, model (position, yaw and rotation, variant, mesh; −1 for no
    mesh is the port's `usize::MAX`), natural-resource growth and removal flag, growth, spawn
    clocks and bucket;
  - the map cells and layer totals.
- **Stage 2:**
  - the sites' state machines, build progress, work registration, clearing count, decal,
    workplace and storage;
  - the piles;
  - the town manager's registries, known goods and values;
  - every work's base fields and the state of the stage 2 kinds;
  - the job manager's id table, category sets, profession map and counters.

What the identity check turned up:
- **Empty entries.** The original keeps a profession's entry in the workplace registry, and in
  the job manager's profession map, after its last workplace or work goes. The load now keeps
  those empty entries too.
- **A null profession.** The job manager can hold a null-profession key; in the port it is
  `usize::MAX`.
- **Key order.** These maps are keyed by object address in the original, so their order is only
  known from the save. The writer keeps the saved order and puts new keys after it.

## Saving from the game

`GameplayLoop::save_state` copies the loaded save, writes the town into it, then writes the
loop's own state:
- the game's Random and the calendar;
- the camera's target, yaw, pitch and distance;
- the environment's controller, with each weather effect;
- the event log (an entry made in the port is formatted as the original stores it: its text);
- the Town Info history and its clock;
- the stock slots and capacity records.

`GameplayLoop::save_file` encodes it and builds the file with the loaded save's metadata. The
`save PATH` script command (native) writes it. Load Save83, run a few seconds, save, and the file
decodes and loads back with every citizen, stack, animal and work.

The preview image and name in the metadata are still the loaded save's.

## Still to write

- Citizens, their action stacks and the other work kinds' claims.
- Wildlife and boats.
- The buildings' component state.
- The environment, event log and history, the stock slots, the Random, the calendar and the
  camera.
- **Entities and objects created or destroyed since the load.** A work made after the load is
  written as a null entry in the job table for now.
- **The entity manager's lists.** The entity list, the five bucket lists and the schedules come
  from the save. The saved buckets also hold the citizens (bucket 1), which the port updates in
  its own pass. The quotas are `[50, 15, 8, 5, 2]` (`+0xa8`), followed by each bucket's cursor
  (`+0xbc`).
- New games that were never loaded from a save.
