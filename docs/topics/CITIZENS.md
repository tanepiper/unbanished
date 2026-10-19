# Citizens

In Banished, the player never controls a citizen directly. Each citizen decides what to do next:
eat, get warm, fetch firewood for the home, go to work, visit the doctor. This page explains how
that decision is made, and how it becomes walking and working. The code is in
`crates/game/src/behaviour/` and `crates/game/src/people/`, and path finding in
[banished-path](../../crates/path/README.md).

<video src="../assets/videos/citizens-market.webm" autoplay loop muted playsinline width="720"></video>

## Three components

A citizen is an entity made from the template `Template/Citizen.rsc`. Three of its components
work together:

| Component | What it does |
|---|---|
| `CitizenComponent` | The needs (hunger, cold, health, happiness), the age and the household |
| `CitizenAIComponent` | Chooses the next activity |
| `CharacterComponent` | Runs a stack of actions: walks, turns, plays animations, carries things |

## Choosing an activity

When a citizen has nothing to do, or when hunger or cold becomes urgent, the AI builds a list of
*candidate* activities, each with a priority. For example:

| Activity | When it is a candidate |
|---|---|
| Eat | Hungry; urgent when very hungry |
| Get warm | Cold; urgent when very cold |
| Emergency work | A fire nearby, for example |
| Fetch clothing | The citizen has no coat |
| Fetch a tool | An adult without a tool |
| Stock the home with food or firewood | The home is short of it |
| Visit the doctor, fetch herbs | Sick, or in poor health |
| Go to school | A child, if there is a school |
| Work | An adult with a job |
| Drop off what it carries | Carrying goods for a long time |
| Wander | Unhappy for a long time, or nothing else to do |

The list is shuffled with the game's random numbers, then sorted by priority, highest first.
Activities with the same priority keep their shuffled order. The AI then tries each activity in
turn, until one of them gives the citizen something to do. Each activity has a cooldown, so a
citizen does not try the same failed thing again at once.

New citizens have no tool and no coat. So the first choices of an adult are: fetch a tool, fetch
a coat, work, and wander.

The shuffle uses the shared game random numbers, and the sort is the C runtime's unstable
`qsort`. Both are reproduced exactly, because the order decides which citizen does what (see
[Exact maths and random numbers](EXACT-MATHS.md)).

## Actions

An activity becomes a stack of *actions* for the `CharacterComponent`. The actions are small
steps, for example:

`MoveTo`, `PlayAnimation`, `PerformWork`, `PickupRawMaterial`, `PickupStoredRawMaterial`,
`DropOffRawMaterial`, `ConsumeRawMaterial`, `CreateRawMaterial`, `CompleteTask`, `CompleteRoad`,
`HuntAnimal`, `RemoveNaturalResource`, `ShowTool`.

"Fetch firewood for the home" becomes: walk to a storage that has firewood, pick it up, walk home,
drop it off. `ShowTool` puts the right tool in the citizen's hand: an axe, a pick, a scythe, a
hoe, a hammer, a fishing rod, a saw, a bucket, a barrow, or the goods it carries.

<video src="../assets/videos/fisher-closeup.webm" poster="../assets/videos/fisher-closeup-poster.webp" autoplay loop muted playsinline width="720"></video>

## Walking

`MoveTo` asks the pathfinder for a path of cells. The pathfinder is an A* search over the map
cells, with a cost for each type of cell. For citizens, the costs are 3 to 16: ground costs 4,
and some of the cell types that buildings and roads make cost 3. A diagonal step costs 1.4 times
a straight one. Water and high ground block walking. Before it searches, the game checks that both ends
are in the same connected *region*, so it does not search for a path that cannot exist.

The citizen then follows the path. Each iteration, it turns towards the next point, moves by the
*root motion* of its walk animation, and is placed on the ground height.

<video src="../assets/videos/labourer.webm" poster="../assets/videos/labourer-poster.webp" autoplay loop muted playsinline width="720"></video>

To choose the **nearest** storage, job or resource, the game does not run a full search for
every candidate. It uses a coarser *sector graph*: the map is cut into 8 × 8-cell sectors, each
sector's walkable areas become nodes, and a quick search over the nodes gives a "path rank"
distance. These distances are cached for 120 seconds of game time.

## Lives

Citizens are born, grow up, go to school, work, form households, have children, grow old and
die. They need food of several kinds, firewood and clothing, and their health and happiness
depend on them. These rules are in `crates/game/src/people/`; see
[CITIZEN-LIFE.md](../CITIZEN-LIFE.md) and
[CITIZEN-NEEDS-AND-HOUSEHOLDS.md](../CITIZEN-NEEDS-AND-HOUSEHOLDS.md).

## How it was proved

The region labels, the A* search and the candidate sort were run in the emulator with many random
inputs and compared with the port: 20 random maps for the regions, 600 searches for A*, and 400
candidate lists for the sort. All results are identical. The tests are `regions_emu`,
`astar_emu` and `candidate_sort` in `crates/game/tests/`.

## Read more

- [CITIZEN-AI.md](../CITIZEN-AI.md): the decision, the action stack and walking, in detail.
- [JOBS.md](../JOBS.md): construction, work and professions.
- [COMMUTE.md](../COMMUTE.md): how the town moves workers closer to their work.
- [IMMIGRATION.md](../IMMIGRATION.md): new citizens and nomads.
