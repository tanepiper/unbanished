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
   were already entity by entity. Left over: the nomads' and boats' CharacterComponent notify
   (their only scheduled component) runs every iteration in `update_nomads` and `update_boats`,
   not on their bucket-1 visit.
3. **One every-iteration set.** Run the bit-`0x04` phase entity by entity in creation order (the
   port's stand-in for the address order).
4. **Deletions.** Queue every deletion and run them after bucket 4.

## Open questions

- The bit-`0x04` set's order is by entity address, which the port cannot reproduce; creation
  order is the closest stand-in (already a Deviation for animals).
- Where FireComponent sits in a building's component array (it is added after the template's
  components, but the exact place is not traced), which decides whether a building's fire runs
  before or after its state machine on a visit.
