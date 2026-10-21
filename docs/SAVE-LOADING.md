# Loading saves

A `.sav` file is decoded completely by `unbanished_formats` (see
[SAVE-GAME-STATE.md](SAVE-GAME-STATE.md)). Loading turns that decoded state into the port's
running game. It is done in stages:

1. **Terrain, map cells and placed entities** (done).
2. **Storage, buildings, construction and the job manager** (done).
3. Citizens (3a), their action stacks and work claims (3b), wildlife, livestock and trader
   boats (3c), and the buildings' own component state (3d): all done.
4. **Environment, weather, the event log, the Town Info history and the town manager's other
   values** (done).

## Starting a load

The original Load dialog (`Dialog/SaveLoad.rsc`) is not ported yet. A load starts from the menu
loop's `load PATH` script command (native only):

```bash
cargo run -p unbanished-app --release -- --boot-capture out.png 20 --script "load save-game/Save3.sav@5"
```

`MenuLoop` parses the file into a `SavedGame` (`crates/sim/src/saved.rs`) and switches, through a
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
- **The town.** `StartingTown::from_save` (`crates/sim/src/town/load.rs`) restores every entity
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

The code is in `crates/sim/src/town/load/construction.rs`. Each building is created as a newly
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

Test: `start::load::stage2_tests::vanilla_saves_restore_construction`, run on Save2 to Save5
and Save83 (a vanilla town in year 22 with 116 citizens). Trader boats have a state machine but
are characters, not sites (stage 3c).
- **Stock counts.** The stock counts the port recomputes from the restored storages
  (`stock_table`) equal the stock manager's saved counts in all 21 slots. For example, Save4
  holds 95 logs, 52 stone, 40 iron, 200 firewood, 30 tools, 1800 food and 10 clothes.
- **Machines.** Every site's machine is consistent: its controller has the saved state at
  the saved index. Save83's bridges showed the port lacked the MultiObject controller (see
  [BRIDGES.md](BRIDGES.md)).
- **Works.** Every registered site work and pile work belongs to its entity.
- **Counts.** The work, planned-road and storage-registration counts match the save.

## What stage 3a restores

The code is in `crates/sim/src/town/load/citizens.rs`. The citizens are restored in the town
manager's registry order (`+0xa0`). Each CitizenComponent (`FUN_140119630`) fills the port's
`Citizen` and `CitizenState` field for field, by offset:

| Offsets | Fields |
|---|---|
| `+0x28` | name |
| `+0x34` | appearance |
| `+0x38`, `+0x3c`, `+0x40` | sex, age, lifespan |
| `+0x44`, `+0x48` | hunger, cold |
| `+0x4c` | reproduction |
| `+0x50..+0x58` | health, health events and timer |
| `+0x60..+0x78` | disease clock, disease, illness or immunity left, spread timer |
| `+0x80..+0x90` | happiness, flags, happiness timer |
| `+0x98` | foods eaten |
| `+0x1f0` | profession |
| `+0xa8`, `+0xac` | tool status and uses |
| `+0x1e0..+0x1e8` | clothing |
| `+0xb0` | supply activity |
| `+0x30` | job |
| `+0x238`, `+0x240` | home and family slot |
| `+0x250` | spouse |
| `+0x258` | guardian |
| `+0x260..+0x270` | the dependents' ring |
| `+0x248` | workplace |
| `+0x1f8` | accident |
| `+0x208..+0x220` | school, education, school time, schooled |
| `+0x228`, `+0x230` | doctor, priest |

- **Educated** is flag `0x400` (`FUN_140116c70`).
- **The entity's other components:**
  - ModelComponent: the body's position and yaw;
  - CharacterComponent: walking direction, heading, speed and the `+0x28` flag;
  - CitizenAIComponent: current and queued activity, cooldowns, urgent flags, loiter and
    carrying times;
  - StorageComponent: the goods carried;
  - StatusIconComponent: the icons.
- **Buildings:**
  - homes' families and fuel (registered homes are the town's, others stay on the site);
  - workplaces' workers;
  - the attendance components' attendees and the town's attendance list.

Action stacks and work claims are stage 3b, below.

The town manager's lists hold either entities (`+0x00..+0x30`) or a component of one (the
sites, healers, cemeteries, happiness and attendance lists). Both resolve to the entity.

Test: `start::load::stage3_tests::vanilla_saves_restore_citizens`. Every citizen is restored.
The relations are consistent: - a citizen with a family slot is in that home's family;
- spouses point at each other;
- dependents name their guardian;
- workplaces and workers agree both ways.

## What stage 3b restores

**Action stacks** (`crates/sim/src/town/load/actions.rs`). In the original, a CharacterComponent
saves its stack array (`+0x48`, top `+0x60`) and its "on empty" action (`+0x58`). Each array
entry is one action with its started flag (`+0x18`). An action linked to an entry (`+0x10`, a
hunter's watch for example) is not saved. Each entry becomes a one-action chain of the port's
`ActionStack` (`ActionStack::restored`).

Every saved action maps onto a port action, field by field:

| Saved action | Fields |
|---|---|
| MoveTo | way, next cell, target, flags, clips, cost mode, following, speed |
| PerformWork, PlayAnimation | clip, duration, elapsed |
| SendComponentMessage | target component from its type id |
| ShowTool | the tool |
| CompleteTask | work id `+0x30`, parameter `+0x34` |
| CompleteRoad | cell, type `+0x24`, built byte `+0x28` |
| ConsumeRawMaterial, PickupStoredRawMaterial, DropOffRawMaterial | storage and goods; a consume without an entity uses what the citizen carries (`FUN_1400cb880` on its own storage); for a drop-off, no storage leaves the goods on the ground and no material drops everything |
| RemoveNaturalResource, AnimateNaturalResource, PickupRawMaterial | the resource or pile |
| CreateRawMaterial | template, cell `+0x44`, batch `+0x40`, workplace, factor `+0x4c` |
| CreateNaturalResource | template, cell, occupant |

A stack holding an action the port cannot rebuild yet starts empty and is counted in the
load log.

**Walks in progress.** The original restores each character's animation from the save's
animation block (`AnimationManager::Serialize`, `FUN_18005d130`), which the port carries as raw
bytes. A restored, started MoveTo therefore plays its walk clip again and faces its next cell on
its first update (a deviation). Without it a walking citizen had no root motion and stood still
for good: on one test save 12 of 17 citizens were frozen after loading, and a worker frozen this
way never looked for work again (issue #4). HuntAnimal and RemoveAnimal wait for the animals (stage 3c).

**Work claims.** The works now carry their citizens:
- the gatherers', builders' and other workers' sets;
- the clearing claims, both resources and cells;
- a road's builder;
- a pile's claimer;
- the production and fishing use points;
- the traders' and vendors' loads;
- the planting and harvest claims.

Every work kind in the samples is restored, among them forester, herbalist, hunting, pasture,
tending, firefighting, fishing, school and chapel staff, trade, market, planting and harvest.
The NaturalResource flag `0x100000` puts a resource back among the claimed ones.

**Found on the way.** In the original, the fishing sequence also messages the FishingComponent,
whose default handler does nothing. The port had left the action out, so its stacks differed
from the original's. `Component::Fishing` now exists and the sequence pushes the message where
the original does.

Test: `start::load::stage3b_tests::vanilla_saves_restore_stacks_and_claims` (Save2 to Save5,
Save83). Every stack is restored at its saved depth. Every citizen listed among a work's workers
has that work as its job (Citizen `+0x30`). That relation spans two separately saved structures.

## What stage 3c restores

The code is in `crates/sim/src/town/load/wildlife.rs`.

**Placing them.** Populations, herds, animals and trader boats have no map footprint. Stage 1
places them as `Wild::place` does, at the cell under their position. Each goes into its saved
bucket, except a boat, which the port updates outside the buckets.

**Their kinds** are loaded from the templates as a New Game loads them: the populations' with
their herds and animals (`wildlife_kinds`), the livestock (`livestock_kind`) and the boats
(`load_boat`).

**The fields:**

| Component | Fields |
|---|---|
| PopulationComponent (`FUN_140054e00`) | cell width `+0x30`, herd blocks `+0x48`, each cell's capacity, count and next count (written in the order `[0], [2], [1]`), clock `+0x60`, cursor `+0x68` |
| HerdComponent (`FUN_140127460`) | its population `+0x28` and block `+0x30`, its animals, moving `+0x48`, target `+0x4c`, rest `+0x50`, regrowth `+0x58` |
| AnimalComponent (`FUN_14010c310`) | home `+0x28`, timer `+0x30`, area `+0x38..+0x3e`, male `+0x40`, age `+0x44`, lifespan `+0x48`, breeding `+0x50`, its flags (wild, hunted) |
| WildAnimalComponent | its herd |
| LivestockComponent | pasture, timer |
| PastureComponent | the kind kept, its animals, the limit `+0x40`, the products gathered `+0x44` |
| BoatComponent (`FUN_140046f30`) | trading post, dock and heading, target, route and index; its state from its state machine |

- **Bodies.** An animal's and a boat's body comes from the ModelComponent and the
  CharacterComponent, with its action stack, as a citizen's does. The animal's scale follows
  its age (`FUN_14010c7f0`).
- **Town lists.** The town's pasture list (`+0x90`) and livestock without a pasture (`+0x80`)
  follow.
- **Actions.** HuntAnimal (its lodge and done flag) and RemoveAnimal (its animal) now rebuild,
  so hunters' stacks restore too.

Test: `start::load::stage3c_tests::vanilla_saves_restore_wildlife`. Every population, herd,
animal and boat is restored. Every herd's animals point back at it, and every pasture's
livestock names the pasture. Save83 has 36 herds, 261 animals (36 in pastures) and 2 boats.

## What stage 3d restores

The code is in `crates/sim/src/town/load/producers.rs`. Each building component's fields go onto
the port's site fields, following the offsets in the port's doc comments:

| Component | Fields |
|---|---|
| Production (ResourceProduce and ConsumeProduce) | disabled flag, selector, recipe weights, produced count |
| Forester | plant and cut toggles |
| Hunting | the hunting time |
| Fishing | registered, water fraction |
| CropField | on, piles, crop, cells (cell, state, crop, pile) |
| Orchard | on, year, tree, slots |
| FruitComponent | ripeness, picked |
| AutoPickup | on, its Pickup works |
| MultiObject | parts, stages |
| Happiness | registered |
| Cemetery | graves |
| Disease | its four clocks, on a pasture, what last grew |
| Destroy | count, target, refund, rebuild, damaged, time |
| Fire | buckets, burn, spread |
| Immigration | arrived, timer |
| Trade | the merchant's goods, the merchant, goods to keep, orders by merchant, automatic trades, mode, clock, held, boat, working |

- **Other components:** trackers, terraform points, tombstones, and the crops' keep-pile flag.
- **Not restored yet:** nomads and tornadoes, which are logged.

**Floors.** The map cells save their floor counts (`+0x2d`). On load, the original's
FloorComponent slot `+0x58` (`FUN_140124a10`) adds every active floor again: one more on each
of its cells, with nothing taken away first, because the mesh handle is not saved. So in the
original the counts grow with every load: a Save83 bridge reads 2 in the earlier Save 47 and
5 later. The load restores the saved counts and then re-adds the active floors in the same way.

This also exposed a port gap. Floors whose FloorDescription does not add them on activation
(`+0x28` false: docks, the school's and the trading post's) were never loaded. In the
original, MultiObject's state 2 adds them when the building is built. The port now keeps every
floor mesh, and its MultiObject state-2 entry adds the parts' floors (see
[BRIDGES.md](BRIDGES.md)).

Test: `start::load::stage3d_tests::vanilla_saves_restore_producers`.
- **Floors.** The floor counts equal the saved ones, plus one under each active floor, on
  every cell (490 floor cells in Save83).
- **Crops and trees.** Growing crops point at live entities, and standing trees carry their
  fruit.
- **Trading posts.** The trading posts' goods resolve.

## What stage 4 restores

- **Environment and weather** (`FUN_140099160`, `Environment::restore`):
  - the weather state `+0x10` and precipitation level `+0x14`;
  - the temperature `+0x18` and the renderer's season values `+0x1c..+0x28` (snow, autumn,
    leaf loss, rain);
  - the annual baseline and bias with their targets (`+0x2c`/`+0x30`, `+0x44`/`+0x48`);
  - the last weather slice `+0x4c`;
  - each rain or snow effect that exists, with its direction and intensity.

  The temperature noise (`+0x34`) is not saved. After a load the original keeps its
  constructor's clock-seeded field until the next reseed (slice 0 or 24), so the weather until
  then cannot be reproduced even in the original. The port keeps its fixed seed (a deviation).
  `Game +0xd0` (disasters) comes from the save.
- **Event log** (`FUN_14009a790`): the six notify flags, and every entry with its colour,
  type, severity, cell, category and icon. The original keeps only an entry's formatted text, so
  `EventEntry::text` holds it and the log shows it as is.
- **Town Info history** (StockManager `+0x150`) and the time to the next sample (`+0x198`).
- **Town manager values:** the visitors' disease risk and how long it lasts (`+0x1a8`,
  `+0x1a0`), the value traded away (`+0x1ac`), the nomads taken in (`+0x1b0`) and the commute
  clock (`+0x190`).

Test: `start::load::stage4_tests::vanilla_saves_restore_environment_and_logs`.
- **Temperature and snow.** The restored temperature is the saved one. The snow cover agrees
  with it (`clamp(32 − temperature)`, `FUN_1400988f0`), which checks the field mapping.
- **Event log.** Every entry is restored with its text (Save83 has 200).
- **History.** The Town Info history is restored (1035 samples per series in Save83).

## Not restored yet

- Nomads and tornadoes.
- The engine-side blocks: achievements, animation states and the audio scene. The port
  rebuilds its own animation and ambience.
- The achievement block (AchievementManager::Persist, engine side) and the scenario goal
  object. No vanilla sample has a goal.
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

The vanilla saves `Save2` to `Save5` and `Save83` are the reference for loading. `Save 47`
lists two mods; `Save200` and `Auto-1` are 2015 saves made with mods.

`Save4` and `Save5` are the same town (Seminola) about 106 game seconds apart. Between them:
- the roads under construction were finished;
- a WoodHouse was completed;
- 17 clear jobs finished, and some trees, rocks and ore were harvested.

`cargo run -p unbanished-formats --example savegame_diff -- save-game/Save4.sav save-game/Save5.sav`
lists the differences.
