# Saving a game

The port saves a town by writing its state back into the game state it was loaded from. The
code is in `crates/sim/src/town/save.rs` and its submodules. The writer then encodes the result
(`GameState::encode`) and builds the `.sav` file (`Save::write`); see
[SAVE-GAME-STATE.md](SAVE-GAME-STATE.md) and [SAVE-FORMAT-AND-RESTORE.md](SAVE-FORMAT-AND-RESTORE.md).

Every field the load reads ([SAVE-LOADING.md](SAVE-LOADING.md)) is written back by the same
offset. A loaded town remembers the saved object each entity and citizen came from
(`StartingTown::save_ids`).

A value the load derives is rewritten only when the port's value differs, so an untouched
object keeps its exact bits. A model's rotation is one: saves show `(0, 0, −sin(yaw/2),
cos(yaw/2))`. A loaded entity that has not moved or turned keeps its saved rotation exactly. A
moved or turned entity, or a new one, gets `FromYawPitchOffset(yaw, 0)`. Older saves (Driscola,
2015) hold rotations with `+0` zeros and other last bits, which the port used to rewrite.

## Checking it against the original

The check is `start::save::tests::vanilla_saves_export_unchanged`. Each vanilla save (Save2 to
Save5, Save83 and Goldfoothill, 565 citizens) is loaded and written straight back. The encoded state must equal the original byte for byte, except for
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

- **Objects made or gone since the load** (`save/create.rs`):
  - **New entities.** A new entity gets its ComponentEntity and one component per description
    of its template that makes one, in the template's order. The constructor (`FUN_1400c8bf0`)
    asks each description's slot `+0x40` for the component class. A UIDescription makes one
    only when its `+0x20` is above 0 (`FUN_14003a040`). Checked:
    `create::tests::templates_predict_the_saved_components`, where every template's components
    in Save5 and Save83 are predicted exactly, in order.
  - **Registering them.** A new entity joins the entity list, its bucket and the schedules
    (list index, bucket slot or −1, bucket, last update time).
  - **Dead entities** leave the list and the bucket, the last entry of each moving into their
    place as the original's lists do.
  - **New works** get an object of their class.
  - **New object indices** follow the highest one used. The tag (`Internal +0x0c`, drawn on the
    factory's Random in the original) is the index.
- **Stage 3a** (`save/citizens.rs`):
  - births (the citizen template's entity, in bucket 1) and deaths;
  - every CitizenComponent field by its offset;
  - the dependents' ring (`+0x260` head, `+0x268` next, `+0x270` previous);
  - the body's model, character, AI, carried goods and icons;
  - the town's citizen list, the homes' families and fuel, the attendees.
- **Stage 3b** (`save/actions.rs`):
  - **Stacks.** Each character's stack is written to its saved length (unused slots null), with
    its top and its "on empty" action. An entry whose saved action at the same place has the
    same class keeps that object, so an untouched stack keeps its bits.
  - **CompleteTask's `+0x28`** is an entity its maker passes on to the work's complete slot
    (`FUN_1400cb270`, `FUN_1400fdc30`). The port does not keep it, so a saved task keeps its
    own and a new one has none.
- **Stage 3c** (`save/wildlife.rs`):
  - populations (`FUN_140054e00`; cells written `[0], [2], [1]`, the herd list at `+0x50` left
    as saved, empty in every sample), herds (`FUN_140127460`) and animals (AnimalComponent
    `FUN_14010c310`, WildAnimalComponent, LivestockComponent);
  - pastures, the town's pasture and unassigned livestock lists;
  - trader boats (`FUN_140046f30`) and their state machine;
  - each body's model, character and stack. An animal's model mesh `−1` stays `−1`; a boat's
    model is not touched.
  - Pointers go to the components the serializers check: a herd's animals are
    WildAnimalComponents, an animal's herd a HerdComponent, a pasture's animals
    LivestockComponents.
- **Stage 3d** (`save/producers.rs`):
  - fruit, trackers (year `−1` as saved), active floors, terraform points, tombstones and the
    crops' pile flag;
  - each building's own components: produce, forester, hunt, fishing, crop field and orchard
    (a cell's crop entity and its NaturalResourceComponent; a tree's FruitComponent too),
    auto pickup, multi-object parts, happiness, cemetery, disease, destroy, fire, immigration;
  - the trading post (`FUN_14005c6b0`). Its kept goods are a map keyed by the material's
    address, written in the saved order.
- Loading now also restores a boat's `flag_28` and a tracker's or orchard's year `−1`.
- **Fishing slots** (`FishingWork`): the citizens are slots indexed by use point (`FUN_1400ef380`
  frees slot `param & 0xf`), with empty slots in between. They were written through the
  address-ordered set rule (`like_saved`), which moved every empty slot to the front. They are
  now written as they are. Goldfoothill showed it.
- **Eating from the citizen's own hands** (`ConsumeRawMaterial`): the 1.0.7 saves hold no entity;
  Driscola (2015) holds the citizen's own entity. Both mean the citizen's own storage
  (`FUN_1400cb880`). The load took the citizen for a storage it could not find, and started 27
  citizens' stacks empty. Both forms now load, and a saved action that held the citizen keeps it.
- **The entity manager's clock** (the first value of its block, the scheduler's time), now also restored on load.

`a_simulated_town_reloads_as_saved` loads Save83 and runs it for a minute, which changes about
15,000 fields and makes about 200 objects. It then saves, loads the result and saves again, and
requires the same state. The only difference allowed is the floor counts the original's load
adds again. That makes a save written by the port a fixed point of load and save.

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
`save PATH` script command (native) writes it. Load Save83, run a few seconds, save. The file
decodes and loads back with every citizen, stack, animal and work.

**Metadata** (`SaveMetaData`, Runtime `0x1800c3d00`):
- **The name** is the town's (`GameState::town_name` is also written back, so a renamed town
  saves under its new name). The Save dialog passes the string at `+0x28` of the object it is
  given (`FUN_140011f90`), which is the town's name in every sample, so this is inferred.
  Quick save names its save with the localized `Autosave` string instead (`FUN_14005e820`,
  `FUN_14009f210`; "Auto Save" in the samples).
- **The preview** is drawn as the original's (`SaveLoad::Draw`, `0x1800c5720`):
  1. Starting a save (`SetSaveSprite`, `SetOperation`) waits for the next frame.
  2. That frame copies the screen. It takes the largest centred part with the save sprite's
     aspect and draws it into a buffer of the sprite's size (172×128 in every sample).
  3. It reads the buffer back as blue, green, red, alpha, and `UpdateSave` then writes the file.
  - The port renders the scene without UI, crops it and scales it bilinearly
    (`saved::preview`; the material's filter is assumed linear, without mipmaps).
  - The original's fourth byte is the frame's alpha, mostly 0 with small values its shaders
    leave; the port writes 0.
  - The Save dialog (docs/SAVE-LOAD-DIALOG.md) and the `save PATH` script command set the save pending, the next draw hands the preview to
    the loop (`GameLoop::save_preview`), and the next update writes the file.
- The extension value (2) and the mod list are the loaded save's.

## New games

`crates/sim/src/town/save/new_game.rs`. A game started with New Game has no saved state, so saving starts from
an empty one:
- **Loop fields:** the New Game settings, as references.
- **Entity manager:** empty, with five bucket lists, quotas `[50, 15, 8, 5, 2]` and cursors 0.
- **Camera:** its transform (rotation and position) is computed from the target, yaw, pitch and distance.
- **Terrain:** the generated map's block, at version 6.
- **Map cells:** no description (`u32::MAX`).
- **Town and managers:** at the schema's defaults. Lists the writers fill entry by entry (weather effects,
  Town Info series, stock slots, capacity records) are sized to the port's state.
- **Achievements:** an `AchievementInstance` for each achievement in the list, with each goal's progress.
  A loaded game reads its instances back (`AchievementManager::restore`, goals matched by reference)
  and writes the progress into the same objects; the five samples write back unchanged.
- **The game's goal** (`+0x150`, met flag `+0x158`): a scenario's `AchievementInstance`, the
  scenario's reference as its achievement. A loaded game keeps its saved instance unless the
  scenario was started from its save (`FUN_14009bfd0`: the loop's scenario is not −1).
- **The rain emitter** (`FUN_140099790`): while a weather effect lasts, its ambient's entity (no
  bucket), the first slot's volume the intensity.
- **Animations:** an empty block.
- **Audio scene:** 4096 empty sound slots.
- **Metadata:** version 3, extension value 2, no mods.

The town export then makes every entity, since there are no saved ids. On top of that, a new game writes:
- **Water** (`fire::Water`): the shore points (`+0x130`, `FUN_1400a43f0`), the wells (`+0xe0`) and the
  8×8 block index (`+0x140`, which the original rebuilds when it is empty, `FUN_140094a50`).
  The points match Save2's and Save3's lists exactly.
- **The camera controller** (`FUN_140107df0`): the zoom target, pitch target and slider, `+0x34` = 1,
  and five empty bookmarks.
- **The environment's references:** `Environment.rsc`, then the climate.
- **The map ambience** (`FUN_1400a57c0`): a `RiverEffect`/`WindEffect.rsc:ambient` entity for
  each emitter (no bucket), and an `Environment.rsc:ambient` entity for each region (bucket 2),
  with its cells and its two slots' effects and volumes. No sound handles (the audio scene is empty).
- **The minimap** (`FUN_1400aacc0`): the terrain image the port draws (`FUN_1400aa610`, read back
  at the save's frame, top row first, `0x00RRGGBB`) and the overlay (`0xAARRGGBB`). Against
  Save2 and Save83 the port's terrain image has the same orientation, and differs by about 14 per
  channel (the known minimap colour difference, docs/MAP-WINDOW.md).

Every save also gets the following:
- **Herds and populations** (`+0xb0`, `+0xc0`).
- **Zones by template** (`+0x160`): each placed entity with a ZoneComponent, from its
  activation, built or not. All five samples write back unchanged with the computed zones.
- **Capability bits:** a created component gets its class's constructor bits (`create::initial_flags`:
  the one value every saved component of the class has). Characters' models get `0x04`, a status
  icon `0x04` while it shows, a particle `0x04` while a slot is on. Every entity's flags are
  its components' low 16 bits together. This held for every component and entity in nine samples.
  A growing tree's `0x10` is not set, as the start's seeded trees have it clear. `a_new_game_saves_and_reloads`
generates the seed-1234 Medium start, runs it for ten seconds, saves it, then loads and saves again.
It requires the same state both times. A real New Game, saved with the `save` command, loads back
with the same town.

## Still to write

- Nomads and tornadoes.
