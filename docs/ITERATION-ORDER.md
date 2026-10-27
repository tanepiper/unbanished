# One simulation iteration: the original's order and the port's

This page describes the order in which one simulation iteration runs every entity's components,
in the original and in the port, and the plan to make the port's order the original's (issue
#22). The original's order is *observed* in `FUN_1400c9560` and the functions it calls; the
bucket and component lists are read from the shipped templates with
`crates/formats/examples/template_info.rs` and `tools/triage/types.py`. Nothing on this page is
*measured* yet: that needs two saves of one vanilla town made by the original a known number of
iterations apart.

## The original

`FUN_14009b850` runs `Graphics::AnimationManager::Update` and then the entity manager's timed
update, `FUN_1400c9560`, once per simulation iteration. That update:

1. advances the manager's clock;
2. applies queued changes to the bit-`0x04` entity set, then runs that set, **entity by
   entity**, in the set's order (a tree keyed by the entity's address, `FUN_1400c90a0`): for each
   entity, every component with capability bit `0x04`, in the template's component order, with the
   iteration's time (slot `+0x40`, `FUN_1400c82b0`);
3. visits buckets 0 to 4 in turn, each from its cursor up to its quota (50, 15, 8, 5, 2): for each
   visited entity, every component with bit `0x08`, in the template's component order, with the
   time since the entity's last visit (slot `+0x48`), stopping if the entity is queued for
   deletion (flag `0x400`);
4. deletes the entities queued for deletion.

### Which bucket each kind uses

| Bucket | Templates |
|---|---|
| 1 | Citizen, Nomad, TraderBoat, Tornado |
| 2 | PopulationDeer |
| 3 | every building and field, every road and clearing order, every raw-material pile |
| 4 | every natural resource, LivestockChicken, LivestockCow, LivestockSheep, WildAnimalDeer, Tombstone |
| 6 (none) | fences, gates, NaturalResourceRock, AnimalShelter, the utility windows |

No shipped template uses buckets 0 or 5.

### Which components run in each phase

Read from each component's primary vtable (the one its constructor stores at the object's start;
`tools/triage/types.py class NAME`), against the defaults `FUN_14011df40` (`+0x40`) and
`FUN_14011df50` (`+0x48`):

| Phase | Components with their own update |
|---|---|
| Every iteration (bit `0x04`, slot `+0x40`) | CharacterComponent (`FUN_14010e990`), ModelComponent (`FUN_140129e30`), ParticleComponent (`FUN_140053490`), StateMachineComponent (`FUN_140130b10`), StatusIconComponent (`FUN_1401310a0`), TornadoComponent (`FUN_140058630`) |
| On a bucket visit (bit `0x08`, slot `+0x48`) | CharacterComponent (`FUN_14010e950`, the action stack's notify), StateMachineComponent (`FUN_140130af0`), CitizenComponent (`FUN_140118340`), CitizenAIComponent (`FUN_1401148b0`), AttendanceComponent (`FUN_140044cd0`), PopulationComponent (`FUN_1400549d0`), HerdComponent (`FUN_1401270c0`), DiseaseComponent (`FUN_1400494f0`), HuntComponent (`FUN_140127c10`), FireComponent (`FUN_140049910`), RawMaterialComponent (`FUN_14012e600`), GrowthComponent (`FUN_140125520`), SpawnComponent (`FUN_140056430`), FruitComponent (`FUN_14004a740`), AnimalComponent (`FUN_14010b8f0`), LivestockComponent (`FUN_140129020`), TombstoneComponent (`FUN_140058130`) |

The state machine's two slots both run `FUN_1400aeb30`: `+0x40` with `scheduled` 0, `+0x48` with
`scheduled` 1. ResidenceComponent, WorkPlaceComponent, HappinessComponent, BuildComponent,
DestroyComponent, ClearAreaComponent, CropFieldComponent, OrchardComponent, PastureComponent,
NomadComponent, BoatComponent, TradeComponent, CemeteryComponent, MarketLocationComponent and the
rest have neither slot: they act through the state machine's state callbacks and messages.
FireComponent is in no template; it is added to a building when it is built, after the template's
components.

So, for example, a citizen's visit runs its CharacterComponent's notify, then CitizenComponent,
then CitizenAIComponent (the template's order: `character`, `citizen`, `citizenai`); a tree's runs
GrowthComponent, then SpawnComponent.

## The port

Before step 1 below, one iteration ran in two calls from `crates/sim/src/simulation/step.rs`:

- `StartingTown::iterate` (`crates/sim/src/town/frame.rs`): the shared animations, the animals'
  characters (bit `0x04`), the clock, the natural resources' models (bit `0x04`), then buckets 0
  to 4 for the entities the seeder knows (growth, spawning, fruit, tombstones, animals, herds and
  the population); citizens and sites are skipped; then the animals' deletions.
- `StartingTown::update_citizens` → `Agents::update` (`crates/game/src/behaviour/agents/update.rs`):
  the citizens' animations and characters (bit `0x04`); every site's state machine (bit `0x04`)
  and then each visited site's; nomads, boats; the visited sites' attendance, piles, hunts,
  blight, fires; tornadoes; burials; the visited homes; then the citizens bucket 1 visited (bit
  `0x08`); then the citizens' deletions.

## What differs

1. **Phases.** The citizens' characters and the sites' every-iteration components run after
   the natural resources' bucket visits; in the original every bit-`0x04` component runs before
   any bucket.
2. **Buckets.** Bucket 4 (natural resources and animals) runs before bucket 1 (citizens) and
   bucket 3 (sites and piles); the original runs 1, 2, 3, 4.
3. **Within a phase.** The port runs one kind at a time (all animals, then all trees, then all
   citizens, then all sites) and, for sites, one system at a time over all visited sites; the
   original runs entity by entity, each entity's components in its template's order.
4. **Deletions.** The port deletes animals after the buckets but citizens and sites at other
   points; the original deletes all queued entities after bucket 4.

## Plan

Each step keeps the game running and passes the quick tier; the compatibility scenarios change
at every step, and each step's commit says why.

1. **Phases and bucket order.** Done. `StartingTown::iterate` runs the animals' characters and
   the models and takes each bucket's visits; `StartingTown::update_citizens` runs
   `Agents::every_iteration` (citizens' characters, sites' state machines, nomads, boats,
   tornadoes), then bucket 1 (`Agents::visit_citizens`), the seeder's buckets 0 to 2, bucket 3
   (`Agents::visit_sites`), the seeder's buckets 3 and 4, then the deletions
   (`Agents::finish`). Kinds stay grouped within a phase.
2. **One bucket, entity by entity.** Done. Bucket 3 (`Agents::visit_sites`) runs each visited
   entity's components in its template's order: disease, fire, the state machine (with a
   residence's state-2 work, `FUN_14012ee40`), hunt, attendance, a pile's age. Buckets 1 and 4
   were already entity by entity, and a nomad's CharacterComponent notify (its only scheduled
   component; boats have none) runs on its bucket-1 visit, among the citizens' in visiting order.
3. **One every-iteration set.** Run the bit-`0x04` phase entity by entity in creation order (the
   port's stand-in for the address order).
4. **Deletions.** Done in part. Destroyed sites, picked-up piles, cleared resources and every
   other deleted entity now leave their buckets at the end of the iteration
   (`StartingTown::remove_dead_entities`). Before, only the seeder's removals, the animals and
   the citizens left them. A destroyed site or pile stayed in its bucket and took one of the
   bucket's visits each time its turn came. After two minutes of Save83, 21 of bucket 3's 225
   entries and 48 of bucket 4's were such entities, so the living ones were visited less often
   than in the original, more so the longer a town ran. The seeder's removals keep their request order
   and the rest go in bucket order; the two orders differ only when two entities of one bucket
   are deleted in the same iteration (a Deviation in `registry.rs`). The components' removal
   slots still run kind by kind.

## Measured on a pair of the original's saves

`Save4` and `Save5` are one vanilla town (Seminola) saved twice by the original. Measured with
`save_random_distance` (an example in `crates/formats`):

| | The original (`Save5`) | The port (`--sim-continue Save4 6360`) |
|---|---|---|
| Entity-manager time | +105,995,760 µs, exactly 6,360 iterations of 16,666 µs | the same |
| Draws from the game's Random | 101,012 | 100,925 |
| Build progress of the two sites under way | 30 and 45 | 30 and 45 |
| Clearing jobs left | 257 | 257 |
| Bucket-2 cursor (no entity added or removed) | 224 | 224 |

The pair is not order-free: `Save5` has a second WoodHouse site at progress 0 that `Save4`
does not have, so the player placed it between the saves. The port cannot know when, so the 87
missing draws include that placement and its effects. The bucket-3 and bucket-4 cursors differ
(15 and 939 against 292 and 929) because entities were removed at different times in the two
runs. The visit loop (`FUN_1400c9560`) and the removal (`FUN_1400c8db0`) are the same as the
port's: the count is read once, a cursor at or past the end starts again at 0, and a removal
swaps in the bucket's last entity without moving the cursor.

## Measured on order-free pairs from the original

On 2026-10-24 the original ran in the project's emulated PC (`tools/reference/README.md`) and made
saves of one vanilla town (Glendontoocook, seed 135789927, Valleys, Small, Fair, disasters on,
Medium) with no orders given between them. The saves are local, in `save-game/vm/`. Each pair was
run in the port with `--sim-continue` for the same number of iterations, and the draws were
counted with `save_random_distance` (*measured*).

| Pair | Audio in the original | Iterations | Original's draws | Port's draws | Missing |
|---|---|---|---|---|---|
| A → B | on | 80 | 1,233 | 1,200 | 33 |
| D → E | off | 90 | 1,380 | 1,350 | 30 |
| E → F, two saves in a row | off | 0 | 8 | - | 8 |

In both pairs every citizen, animal, tree, building and pile is identical in the port and the
original. The only objects that differ are the ambient emitters' fade volumes and seven
achievement goals, and in A → B one animal's `animating` flag. So the order of the every-iteration
phase (step 3) changes nothing that a save holds over these 80 and 90 iterations.

The missing draws come after the last iteration of each pair. Had they come between iterations,
every later walk speed and deer wander target would have been drawn from other numbers, and those
all match. So they come from what happens between the game running and the save being written.
The pair with no iterations measures part of it: opening the Save window from the game menu and
saving draws 8. The other 22 to 25 come with opening the game menu while the town runs. They
stay when XAudio2 cannot start (`UNBANISHED_VM_AUDIO=off`; `Audio::Device::Startup` at
`1800e0cd0` carries on without a device), so they do not depend on the sound device. They are
the UI clicks' sounds: `Scene::Update` draws twice from the game's Random for each sound that
starts, and the pause menu leaves UI sounds running, so each click draws 4 (*inferred* from the
counts; see [RANDOM-DRAWS.md](RANDOM-DRAWS.md), "The audio scene"). The port now makes these
draws too. Within the
iterations the port made the same draws as the original, in the same order (*inferred* from the
matching towns).

## Measured on a long pair with a load

On 2026-10-27 the original loaded `Save83` (Fort Ponchartrain, 116 citizens, year 22, no mods)
in the emulated PC with `UNBANISHED_VM_AUDIO=off`, ran without orders and was saved from the
game menu (`save-game/vm/Fort-1.sav`) (*measured*).

| | The original (`Fort-1`) | The port (`--sim-continue Save83 5480 --clock 1793116942`) |
|---|---|---|
| Entity-manager time | +91,329,680 µs, exactly 5,480 iterations | the same |
| Draws from the game's Random | 137,107 | 154,706 (the same at `--speed 1` and `--speed 10`) |
| Temperature | `0x4252ad49` | `0x4252ad49` |
| Weather state and level | 0 (rain), 5 | 3, 3 |

The original seeds a loaded game's temperature noise from the clock (see
[SAVE-LOADING.md](SAVE-LOADING.md)). With that clock the port's temperature is the original's
to the bit, so the noise is not what separates the two towns. The port makes 17,599 draws more,
and the weather state drawn at slice 27 or 28 already differs, so the two runs part somewhere in
the 5,480 iterations. Where is open: a chain of shorter pairs from one session of the original
will place it.

The autosave cannot make these pairs in the emulated PC. Its timer counts the frames' time
(`FUN_14005ea80`), and the emulated PC runs about one frame a second, so five minutes of frames
take hours.

## Measured draw by draw in CrossOver

The original also runs in CrossOver (bottle `Steam`, `xaudio2_7` disabled, so it plays no
sound). There GameFrame's `winegdb` stops at the Runtime's `Random` entries and records each
draw's caller and the generator's index (*measured* on 2026-10-27). The Runtime loads at
`0x6fffe6db0000` in that bottle, not at its preferred base; the Application stays at
`0x140000000`. Plain `Rand()` is left out of the breakpoints, because `Object::Internal::Internal`
(Runtime `0x1800c2580`) draws it from a separate `Random` for every object made, thousands in a
load. The game's own plain draws show as gaps in the index instead. The original's list from an
autosave of a busy town (`04-Auto`, 57,130 draws) was compared with the port's
(`--sim-continue … --game --draws`, `tools/triage/rngdiff.py`).

| Draw | What differs | Cause |
|---|---|---|
| 18 | The original draws `Rand(15)` (`0x1400a98d2`, the music player) and then 16 plain draws | Without a sound device the music player and `Scene::Update` still draw from the town's Random. `--sim-continue` runs no audio, so it parts here at once; `--game` runs the whole game loop and makes these draws too |
| 31 | The port starts fewer ambience sounds on the first frame | Fixed in part: the port's listener was on the ground. It is at the eye's height (see [AUDIO.md](AUDIO.md)), *measured* at 105.32 for this save's camera |
| 31 | The original starts 7 sounds on the first frame (river, forest, field, wind, rain, the crowd and a farmer's hoe at (65.5, −2.5)); the port starts 5 and the crowd one frame later | Open: the original restores the working citizens' tool sounds with the save and hears the crowd at once; its ambience volumes also differ (wind 0.975 against 1, rain 1 with two emitters against 0.06 with one) |
| 586 (sound draws left out) | The original's gatherer search (`FUN_1400f3880`) ends one attempt earlier than the port's | The numbers differed, not the search: with the original's sound draws replayed (below) the search is the same |
| 815 (sound draws replayed) | `Activity::Loiter` (`FUN_14010fa30`) draws `Rand(125)` in the original and `Rand(124)` in the port | The citizen picks one of 48 objects near it (`Rand(48)` the same on both sides), then a cell around it. The original keeps the 48 in a tree keyed by each object's `Object::Internal` address; the port keeps them in creation order. Of the port's 48, those at places 0 and 25 have 125 cells around them; the port took place 38 (124). So the two orders differ here (a Deviation) |
| 1,025 (pick 815 set to place 0) | A citizen whose gatherer search found nothing decides again 5 iterations earlier in the port | Fixed. The search's wait (`FUN_1400c7960`) sets an "on empty" chain that calls the AI back. The original's SendComponentMessage action (start `FUN_1400d03c0`) calls the component at once, inside `ActionStack::RunOnEmpty` (`FUN_1400ca080`), which then deletes the chain at `+0x10`. A chain the AI set during that call goes with it. So a decision made from the "on empty" message ends with no message, and the next decision waits for the citizen's scheduled update. *Measured* with breakpoints on `CitizenAI::Choose` and the wait: decisions 16 and 21 or 22 frames apart in turn (frames 13, 34, 50, 72, 88…), which the port now gives too (13, 34, 50) |
| 3,511 (pick 815 set to place 0) | The original eats from a home's storage first (13 `Rand(7)` and one `Rand(6)` in `Storage::find_item`, `FUN_140094410`), then makes the citizen's choice; the port makes the choice with no meal | Fixed. The citizen was in the middle of a meal when the game saved. A saved SendComponentMessage holds the entity whose component receives it (`+0x20`) and the value it gives (`+0x28`). The port read the receiver, the citizen itself, for a Citizen message, so the loaded meal had no storage. It reads the value now |
| 3,531 (pick 815 set to place 0) | `Activity::Loiter` draws `Rand(105)` in the original and `Rand(118)` in the port | Another pick from the address-keyed set (`Rand(31)` the same on both sides), as at draw 815 |

To compare the simulation alone, `tools/triage/rng_split.py` splits the original's list. It
keeps the draws to compare and writes, for each, how many sound draws the original made before
it: the gaps in the index and the music player's `Rand(15)`. `--sim-continue --draws FILE
--replay-draws SKIPS` makes those draws in the port before each recorded draw, so both
generators give the same numbers to the simulation. With that, the port matches the original
for 814 draws of the simulation (about 20 iterations of this town). With the Loiter pick at
draw 815 set by hand to one of the two places that give the original's draw (a check only; the
port keeps creation order), it matches for 3,530 draws.

The particle systems draw from their own `Random` in both, so the port's list leaves them out
too. Each load of the original takes its own path after the first frames: its sound draws
depend on the frame times, and a later load of the same save drew other candidate cells for the
same search. So a search's cells must be read in the same run as the draws.

## Open questions

- The bit-`0x04` set's order is by entity address, which the port cannot reproduce; creation
  order is the closest stand-in (already a Deviation for animals).
- Where FireComponent sits in a building's component array (it is added after the template's
  components, but the exact place is not traced), which decides whether a building's fire runs
  before or after its state machine on a visit.
- Step 3's address order: the order-free pairs above show no difference in what a save holds over
  90 iterations. The long pair above differs after 5,480 iterations, for a cause not yet found.
- A pair made with a known number of clicks, to measure the 4 draws per click directly. The
  clicks in the emulated PC register only in part, so the counts above fit but are not
  measured click by click.
- The order of `Object::Internal` addresses in a loaded game. It decides `Activity::Loiter`'s
  pick (draw 815 of the CrossOver pair) and the other address-keyed trees. Whether a load from
  the main menu right after start gives an order the port can reproduce (for example the
  save's order or its reverse) is not measured.
- The sounds the original starts on the first frame after a load (the crowd and the restored
  tool sounds) and its ambience volumes.
