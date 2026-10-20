# Save files

Un-Banished reads and writes the save files of Banished 1.0.7. A town saved by the original
loads in Un-Banished, and the save folder is the same (`Documents/Banished/Save`). This page
explains what a save file contains. The readers are in
[unbanished-formats](../../crates/formats/README.md) (`save`, `savegame`, `savegame_schema`).

## The container

A `.sav` file has a small header and two payloads:

| Part | What it contains |
|---|---|
| Header | A format version (2), the number of payloads (2), a check value, and the offset and size of each payload |
| Payload 0: metadata | The town's name, the preview picture shown in the Load dialog, and the list of mods that were on |
| Payload 1: game state | Everything else: the whole simulation |

Both payloads are block streams compressed with LZF, the same format as the packages (see
[Game data files](GAME-DATA.md)). The check value is a CRC-32 over the payload table and the
payloads; `Save::parse` checks it, and `Save::write` writes it.

A large town's game state is about 25 to 33 MB when it is expanded.

## The game state

The game loop writes the game state in one pass. In order:

1. a table of references to resources (templates, profiles, materials), so that later parts can
   refer to them by a number;
2. the settings: the terrain, climate and start condition, the seed and the map size;
3. **the game's random number generator**: its index and its 624 words, so the simulation goes on
   with exactly the same numbers;
4. the town name and the scenario goal;
5. the achievements, the animations and the sound scene;
6. the camera;
7. the **entity manager**: every entity with all its components, and the update buckets with
   their positions;
8. the terrain: heights, the surface image, the water and the decals;
9. the town manager, the gameplay map (one 48-byte record per cell), the job manager and the
   tools;
10. the calendar, the event log, the environment (month and weather) and the mini-map.

Objects refer to each other with *internal pointers*. The first time an object is reached, it is
written in full, where it is reached. After that, only its number is written. So there is no
separate table of objects; the order of the file is the order in which the game walks its data.

## How the port reads and writes it

`unbanished_formats::savegame` decodes every block of the game state. For each kind of object, it
has a *schema* (in `savegame_schema`) that follows the original's serialiser field by field. Two
checks prove it:

- every block of every sample save is read to its last byte;
- writing the decoded state back gives exactly the original bytes.

Saves from a 2015 build of the game also load. Those builds wrote all text as UTF-16, and the
port keeps track of that so it writes them back the same way.

Loading a save into the running game happens in stages: the terrain, the map cells and the
placed objects; then storage, buildings, construction and jobs; then the citizens, their actions,
animals and boats; and last the environment, the weather and the event log. See
[SAVE-LOADING.md](../SAVE-LOADING.md).

## Tools

```bash
cargo run -q --release -p unbanished-formats --example savegame_check -- save-game/*.sav
cargo run -q --release -p unbanished-formats --example savegame_same -- a.sav b.sav
cargo run -q --release -p unbanished-formats --example savegame_fields_diff -- a.sav b.sav
```

`savegame_same` and `savegame_fields_diff` are used to prove that a change to the engine does not
change the game: two versions play the same scenario and must write the same game state. See
[Compatibility between versions](../COMPATIBILITY.md).

## Read more

- [SAVE-FORMAT-AND-RESTORE.md](../SAVE-FORMAT-AND-RESTORE.md): the container and the header.
- [SAVE-GAME-STATE.md](../SAVE-GAME-STATE.md): the game state, block by block.
- [SAVE-WRITING.md](../SAVE-WRITING.md) and [SAVE-LOADING.md](../SAVE-LOADING.md): saving and
  loading in the game.
- [SAVE-LOAD-DIALOG.md](../SAVE-LOAD-DIALOG.md): the Save and Load dialogs.
