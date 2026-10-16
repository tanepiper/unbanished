# Loading saves

A `.sav` file is decoded completely by `banished_formats` (see
[SAVE-GAME-STATE.md](SAVE-GAME-STATE.md)). Loading turns that decoded state into the port's
running game. It is done in stages:

1. **Terrain, map cells and placed entities** (done).
2. **Storage, buildings, construction and the job manager** (done).
3. Citizens, their work, and the wildlife.
4. Jobs, trade, events and the rest of the managers.

## Starting a load

The original Load dialog (`Dialog/SaveLoad.rsc`) is not ported yet. A load starts from the menu
loop's `load PATH` script command (native only):

```bash
cargo run -p banished-app --release -- --boot-capture out.png 20 --script "load save-game/Save3.sav@5"
```

`MenuLoop` parses the file into a `SavedGame` (`apps/game/src/saved.rs`) and switches, through a
loading screen, to `GameplayLoop::from_save`.

## What stage 1 restores

- **Settings.** `WorldParams` is rebuilt from the loop's fields:
  - the climate, start condition and terrain references;
  - the seed, the size and the town name;
  - the flags (`Game +0xd0`, also used as `Construction::game_flags`);
  - the scenario.
- **The game's Random**, from the saved index and state (`Random::from_parts`).
- **The calendar**, from its year and time.
- **The camera** (`FUN_14009dd90`): its target, yaw, pitch and distance.
- **The world.** `World::from_save` uses the saved MapComponent:
  - heights and surface;
  - terrain material;
  - river paths;
  - the river and lake mesh.

  Nothing is regenerated. The terrain profile is read only for its materials.
- **The town.** `StartingTown::from_save` (`apps/game/src/start/load.rs`) restores every entity
  that has a MapComponent:
  - its template;
  - its cell and footprint (the MapComponent's `x`, `y` is the footprint's corner);
  - its registration bucket (the schedule's third value, `Entity +0x1f`);
  - its model's variant, mesh, position and yaw (the model block's float after the position is
    the yaw in degrees);
  - its natural-resource growth and flags (`0x100000` allows negative growth, `0x200000` means
    removal was requested);
  - the GrowthComponent's growth, maximum and timer;
  - the SpawnComponent's timers.
- **The map cells** (`FUN_14001b570`):
  - type, occupant and stamped object;
  - the previous template;
  - the layer counters (`+0x1c..+0x24`, `+0x26`) and their totals;
  - the flag byte, road kind, road built and growth byte.

Test: `start::load::tests::vanilla_saves_restore_their_placed_entities`, run on the vanilla
`Save2` to `Save5`. It checks:
- every entity with a footprint is placed at its saved cell;
- every stamped cell points at an entity whose footprint covers it;
- the growth and spawn state carried over.

They load and run for 120 simulated seconds without error.

## What stage 2 restores

The code is in `apps/game/src/start/load/construction.rs`. Each building is created as a newly
placed one is (`add_site`: its storage, workplace and site) and then takes its saved fields:
construct, then deserialize, as the original's load does. It is not queued for activation;
its saved state machine already holds its state.

- **State machine** (StateMachineComponent `+8..+0xb`): state, index, pending state and
  transition. The interface is the last controller holding the state.
- **Complete.** Inferred: past the clearing (0, `0x15`, 1) and building (8) states.
- **BuildComponent:** progress and stages.
- **WorkComponent:**
  - `+0x28` work id. It can be stale: the original leaves it after the manager removes a
    finished work, and so does the port.
  - `+0x30` most workers.
  - `+0x2c` kept priority (`i32::MIN` means none).
- **ClearAreaComponent:** the count of objects to clear.
- **DecalComponent:** inferred, a decal with alpha 1 or a switched material is recorded as
  having left setup.
- **WorkPlaceComponent:** its profession and the enabled flag.
- **StorageComponent:**
  - items (material, count, reserved);
  - capacity `+0x38`;
  - accepted flags `+0x3c`.
- **Piles** (RawMaterialComponent): material, count, age and Pickup work. A pile's
  Pickup state comes from the work.
- **Town manager registries:**
  - the storages (`+0x00`) and markets (`+0x10`) join as their state-2 entry has them join;
  - the site list (`+0xd0`), residences (`+0x20`), healers, cemeteries and happiness
    registrations, and trading posts;
  - workplaces by profession (`+0x100`);
  - known materials (`+0x110`) and known crops and trees (`+0x120`).
- **Stock manager** (the town block's slots `+0x170`): counts, previous counts, limits,
  cooldowns, the used and made counters, and the capacity records.
- **Job manager** (`Jobs::restore`):
  - the last id, the next priority and the pass;
  - every work with its id, priority, cell and patience clocks;
  - the categories it was filed under, and its profession;
  - the priority index, rebuilt from the works.

  Kinds restored: BuildRoad (and the map's planned-road table), BuildStructure, ClearArea,
  GatherRawMaterials, Pickup and ConsumeProduce. Other kinds are logged and skipped until
  stage 3.

Citizens are not restored yet, so whatever refers to one is dropped: work claims and workers,
a pile's claimer, a road's builder. The works stay registered and free.

Test: `start::load::stage2_tests::vanilla_saves_restore_construction` (Save2 to Save5).
- **Stock counts.** The stock counts the port recomputes from the restored storages
  (`stock_table`) equal the stock manager's saved counts in all 21 slots. For example, Save4
  holds 95 logs, 52 stone, 40 iron, 200 firewood, 30 tools, 1800 food and 10 clothes.
- **Machines.** Every site's machine is consistent: its controller has the saved state at
  the saved index.
- **Works.** Every registered site work and pile work belongs to its entity.
- **Counts.** The work, planned-road and storage-registration counts match the save.

## Not restored yet

- Citizens, wild animals, herds and livestock (stage 3).
- Producers' and services' component state: ConsumeProduce and ResourceProduce weights,
  crop fields, orchards, fishing, the attendance lists, homes. Also the work kinds those
  register (stage 3).
- The ambient sound emitters. The port rebuilds map ambience from the terrain, as a New Game does.
- Floors (`+0x2d`, bridge decks).
- The save's own scheduler order. Entities are registered in entity-manager order.
- Roads under construction. Save5's finished dirt road draws from the restored road bytes;
  Save4's roads still being built (`BuildRoad` work, cells marked `0x80`) wait for stage 2.

## Mods

The port does not load mods. `Save0`, `Save1` and `Auto` were saved with mods (their metadata lists
them, and a warning is logged on load):
- their terrain (`Plains`) and start conditions (`RK*`) fall back to Valleys and Medium;
- every entity whose template comes from a mod is skipped and counted in the load log.

The vanilla saves `Save2` to `Save5` are the reference for loading.

`Save4` and `Save5` are the same town (Seminola) about 106 game seconds apart. Between them:
- the roads under construction were finished;
- a WoodHouse was completed;
- 17 clear jobs finished, and some trees, rocks and ore were harvested.

`cargo run -p banished-formats --example savegame_diff -- save-game/Save4.sav save-game/Save5.sav`
lists the differences.
