# Save game state

This covers the second payload of a `.sav` file, the game state (see
[SAVE-FORMAT-AND-RESTORE.md](SAVE-FORMAT-AND-RESTORE.md) for the container). The game loop writes
it in one pass (`FUN_14009af60`). The port decodes every block of it in
`crates/formats/src/savegame.rs`, with one schema per object type in
`crates/formats/src/savegame_schema.rs`.

All five sample saves decode completely, and each block is consumed exactly. `GameState::encode`
writes the state back, and the result matches the original bytes exactly. Test:
`crates/formats/tests/savegame.rs` (local only, since `save-game/` is git-ignored). It also checks:
- the decoded map cells, heights, seed and event log against the older scanning parsers in
  `save.rs`;
- that every object pointer anywhere in the state is defined.

To check saves by hand:

```bash
cargo run -q --release -p banished-formats --example savegame_check -- save-game/*.sav
```

## Primitives

- **Block.** `0xAAAAAAAA`, a `u32` size, a `u32` version, the payload, then `0xBBBBBBBB`. The size
  counts the version, the payload and the terminator.
- **Strings.** A `u32` length (including the NUL) with bit 31 set for 8-bit text, then the
  bytes. `String::Serialize`.
- **External pointers** (`ExternalPtr`, reference mode). A `u32` id into the reference table at
  the start of the state; `0xFFFFFFFF` means none. `crs::Reader::external_ref` resolves the id
  once `set_reference_ids` is given the table.
- **Internal pointers** (`InternalBasePtr::Serialize`). A present byte, then:
  - the `i32` factory index;
  - the `u32` tag;
  - the `u32` type id (`crc::type_id` of the class name).

  The first time an index appears, the object's own serializer follows inline (vtable `+0x18`).
  Later appearances are references only. Objects therefore appear where they are first reached,
  and there is no separate object table.
- **Bools.** One byte. Some bools in the samples are uninitialised memory (`0x48`, `0x80`). The
  game reads them as true. The decoder keeps such a byte as `Value::I8`, and a decal keeps its
  `flag_bytes`, so writing reproduces it.
- **Time.** An `i64`.
- **Vector3.** Three `f32`.
- **Transform.** Four `f32` for rotation, then three for position.
- **Object::Type.** A `u32`.

## Order

1. **Reference table** block. A count, then `(u32 id, string path)` pairs.
2. **ResetPersistState** block (8 bytes).
3. **Settings.**
   - Climate, start condition and terrain as external pointers.
   - Seed, then the size index.
   - Flags (`+0xd0`, from version 1) and the scenario (`+0x15c`, from version 3).
4. **The game's Random.** An index, then 624 `u32`s.
5. **Town name**, then `bool` goal done, then the goal (an internal pointer).
6. **Engine blocks**, kept as raw bytes (`GameState::achievements`, `animations` and
   `audio_scene`):
   - `AchievementManager::Persist`: a count, then achievement objects;
   - `AnimationManager::Serialize`: a count, then playing animations;
   - `Audio::Scene::Serialize`: a count, then sounds, then five floats.

   These are cosmetic, or tracked apart from the town. They can be written back verbatim.
7. **Camera** (`FUN_14009dd90`). A transform, a target `Vector3`, then three floats. Uses the same
   reader as the menu scenes.
8. **Entity manager** (`FUN_1400c9970`):
   - the time;
   - every entity, as internal pointers whose first appearance defines the entity and its
     components;
   - five update buckets with their two `i32` rows;
   - one `(i32, i32, i32, Time)` schedule per entity.
9. **Terrain** (`FUN_1400b59a0`, v6). Heights, the surface image, the water mesh, the rivers and
   the decals. Uses the reader shared with the menu scenes (`menu_scene::read_map_component`).
10. **Town manager** (`FUN_140094c30`, v6). Schema `TOWN`:
    - registries by kind (citizens, healers, pastures, cemeteries, wells, livestock, herds,
      diseases, builds and others);
    - workplaces by profession, and entities by description;
    - the statistics records;
    - two times and a float.
11. **Map** (`FUN_1400a45d0`, v4). The size, then `size²` cells, then nine shorts. Each cell
    (`FUN_14001b570`) holds:
    - two internal pointers;
    - a description;
    - nine flag bytes, then a `u16`;
    - six bytes, of which `[1]` is the cell type.

    It has its own struct (`GameMap`) because of its size.
12. **Job manager** (`FUN_1400fe710`, v3). Schema `JOBS`:
    - 15 priority lists of `(key, work)` (14 before version 3);
    - work by profession;
    - three counters.

    `Work` objects are often defined here first.
13. **Tool manager** (`FUN_140107df0`, v2). Schema `TOOLS`.
14. **Calendar** block. An `i32`, then a Time.
15. **Event log** (`FUN_14009a790`, v1, written from game version 2). Schema `EVENT_LOG`: six
    notify flags, then the entries.
16. **Environment** (`FUN_140099160`). Schema `ENVIRONMENT`:
    - the climate and month;
    - the weather state;
    - per weather effect, a present byte and two floats when present.

    The number of effects comes from the climate and is not written. The schema reads effects
    until the block ends (`Op::Rest`).
17. **Minimap** (`FUN_1400aacc0`). Two texel arrays, then the image size.

## Schemas

- **Ops.** A schema is a list of ops: plain fields, external and internal pointers, counted lists,
  arrays counted by an earlier field, nested blocks, version conditions (`If`), flag conditions
  and `Rest`. The same list drives the writer (`Encoder`). It walks each schema alongside the decoded
  fields and writes each object inline the first time it is referenced, in the same order as the
  decoder.
- **Field names.** Each field is named after its in-memory offset (`value_4c`) where its meaning
  is not yet known.
- **Shared preambles:**
  - `BASE`: the component block of entity, flags and description (`FUN_14011de80`).
  - `CONTROLLER`: an empty block.
  - `RESOURCE_PRODUCE`: base, controller, then a disabled flag.
  - `ACTION`: a started flag.
  - `WORK`: entity, WorkComponent, worker counts and times (`FUN_1400fc8d0`, v3).
  - `AUTO_PICKUP_WORK`: `WORK`, then an empty block.
- **Finding a serializer.** Use the vtable that the class's factory installs last. The
  constructor's last vtable store is not reliable: ControllerComponent subclasses have a second,
  state-machine vtable. Fishing, Forester and Herbalist share a constructor and get their own
  vtable only in the factory.

## Gaps

- **Engine blocks.** The achievement, animation and audio-scene blocks are carried as raw
  bytes. To decode them, the next serializers to follow are:
  - Runtime `FUN_18000a2e0` (achievements);
  - `FUN_18005d130` (animations);
  - `FUN_180053110` (audio-scene sounds).
- **Old versions.** Older-version branches are in the schemas where they were simple, but only
  the samples' versions have been exercised. Map versions below 3 are rejected.
- **Real-game load.** `Save::write` builds the whole `.sav` file (see
  [SAVE-FORMAT-AND-RESTORE.md](SAVE-FORMAT-AND-RESTORE.md)), but no written file has yet been
  loaded by the real game.
- **Loading.** The decoded state is not yet loaded into the port's simulation.
