# Separate towns: a town for each player on one map

Step 7 of [MULTIPLAYER-PLAN.md](MULTIPLAYER-PLAN.md). This page is the design and the order of
work; the two first slices are on the `fix/stock-limit-command` branch (fork).

This is the one part of the plan with nothing to trace. The original game has no multiplayer and
no separate towns, so the behaviour below is **our design**: it is marked *assumed* or *inferred*
where it is a choice, and the only hard requirement is that a single-player game and a shared-town
multiplayer game stay exactly as they are (`compare.sh`, the probe baselines, `lockstep.sh`).

## What each player gets, and what they share

In the mod's other mode every player has their own town on the same terrain. That splits the
town's state in two:

| Per player | Shared |
|---|---|
| citizens, their households and their lives | the map's heights and cells (floors, regions, trees, minerals) |
| placed entities: houses, workplaces, fields, stockpiles | wild animals, herds and populations |
| the job market, professions and workplaces | the calendar, the shared `Random`, the environment and weather |
| stock: `materials`, `storages`, `storage_sites` | natural resource seeding and scheduled resources |
| stock limits and warnings, the event log, statistics | disasters and tornadoes |
| the camera, open windows, window settings | achievements? (assumed per player, as single player) |

The stock limit and the event log's notify flags are today one per game (`Simulation::
stock_warnings`, `Simulation::events`). In a separate towns game they are one per player.

## The design

1. **Ownership.** `owner: u8` on `Placed` (one per placed entity) and on `Citizen`, 0 for the
   first player. A single-player game never sets anything but 0. The owner is set when the
   command that creates the entity runs, from `Simulation::acting_player`, and it is what every
   pick below filters on.
2. **Who acts.** `Simulation::acting_player` is already carried by every command: the game loop
   knows the local player's number (`GameplayLoop::player`, 0 outside lockstep), `execute_from`
   hands another player's command on as theirs, `Watched::execute` passes the server's player,
   and a recording keeps each command's player. A command that places, builds, drafts a worker
   or sets a limit acts for that player.
3. **Per-player context.** The systems that hold per-player scalars in one place swap them per
   player while they run: the agents' manager at `Agents::sim`/`with_people`/`think`, and the
   stock manager. Each player's town is walked in player order, so the random draws stay in one
   order for every computer.
4. **One job market, filtered at the sort.** There is one `Jobs`; `Jobs::sort`
   (`crates/game/src/work/jobs/offers.rs`) is the single place a job offer is chosen, so it
   filters on the offer's owner there rather than in every caller. Same shape for stock: one
   `StockManager`, filtered on the storage's owner.
5. **Save.** The vanilla save has no owner field and must not grow one (1:1 with the original's
   format). Ownership is written beside it — see the open question below — and is part of the
   digest, unlike the camera and the window settings.
6. **Digest.** Every player's save must hash the same, so ownership is in it and the camera is
   not, as today.
7. **The lobby.** `Message::Start` says which mode the room is: shared town or separate towns.
   Every computer must run the same mode; a player whose build has no separate towns refuses
   the room instead of desyncing.

## Where the work is

The size comes from how often the simulation reads the five collections that become per player.
Measured on 2026-11-17 (`.rs` files, matches of `.placed`, `.citizens`, `.materials`,
`.construction`, `.wildlife`, …):

| Area | Reads | What it is |
|---|---|---|
| `crates/game/src/tools` | 575 | the commands that place and build: they act for `acting_player` |
| `crates/sim/src/town` | 471 | the town's own frame, loading and saving: the owner travels with every record |
| `crates/game/src/nature` | 231 | cells and wildlife: shared, so mostly no change |
| `crates/game/src/behaviour` | 209 | citizens' AI: picks filtered by owner |
| `crates/game/src/town` | 178 | sites, building, clearing, piles, storage |
| `crates/game/src/gameplay` | 174 | tools, selection, the HUD |
| `crates/game/src/production` | 141 | farms, orchards, quarries: the owner's land |
| `crates/game/src/people` | 136 | citizens, needs, households |
| `crates/game/src/ui` | 101 | windows: the local player's town only |

The rule that keeps this tractable: a **pick** (a job, a storage, a free citizen, a target) is
filtered by the acting player; a **lookup by id** (an entity the caller already holds) is not.
Rendering and picking with the mouse read the local player's town plus every other player's
entities, drawn but not selectable.

## Phases and how each one is checked

| Phase | Work | Check |
|---|---|---|
| 1 | A resource limit and an event-log notification are `PlayerCommand`s | done: `crates/sim/tests/setting_commands.rs`, quick tier |
| 2 | Every command says whose it is, through the game loop, lockstep, the server and recordings | done: `apps/game/src/gameplay/tests/commands.rs`, `crates/session/src/recording.rs`; a new-game recording made by the build before it has the same entries and replays identically |
| 3 | `owner` on `Placed` and `Citizen`, set from `acting_player`, feature `unstable-separate-towns` | a two-owner headless town: two houses keep their owner through a save; single-player `compare.sh` identical, owners all 0 |
| 4 | Stock per owner: `materials`, `storages`, the stock manager, limits and warnings | two towns' stock never mix; a player's limit does not apply to the other's |
| 5 | Jobs per owner: `Jobs::sort`, professions, workplaces | a citizen takes only its owner's offers; the lockstep test's digests agree |
| 6 | Citizens and agents per owner: the swap at `Agents::sim`/`with_people`/`think` | two towns simulate apart, and the shared `Random` draws in one order |
| 7 | Drawing and picking: every owner's entities drawn, only the local player's picked | the viewer's captures; the probe's scenarios unchanged for single player |
| 8 | Save and resync: ownership written beside the save, sent with `Message::Save` | a save of a two-town game reloads with the same owners; a resync agrees |
| 9 | The lobby: the mode in `Message::Start`, refused by a build without it | `tools/compat/multiplayer.sh` with both modes |
| 10 | The end-to-end test: a room of two separate towns over a simulated network | `tools/compat/lockstep.sh` gains a separate-towns setting; every digest agrees |

Phases 3 to 6 are one change in spirit — the data model and its filters — and each must end with
the single-player comparison identical, or it does not land.

## Open questions

- **Where ownership is saved.** A vanilla save is the original's format, so an owner cannot ride
  in it. Two ways: a sidecar file next to the save (simple; the host must send it with
  `Message::Save`, which the protocol already allows) or a second block in the container the
  engine writes around the saved game. *Assumed*: sidecar until the container is traced.
- **The start.** `StartingTown::start` is one point. A separate towns new game needs one start
  area per player. The map generator's scenario has four (docs/MAP-GENERATION.md); *inferred*
  from the four corners the original's scenarios use.
- **Floors and clearing.** Each player's floors are on the shared cells, so a player sees another
  player's roads and cleared land. *Assumed*: shared, as the mod's map is one map.
- **Wildlife and disasters.** Shared: whoever hunts takes from the same herd, and a tornado
  damages every player's buildings where it passes. *Assumed*.
- **Trade between towns.** The shared-town trade is with travelling merchants; between players
  nothing exists yet. *Open*: out of scope for the phases above.
- **Memory.** N towns is N citizen lists in one process; the browser's 4 GB heap is the limit,
  so the lockstep test must measure a two-town game's peak.

## For the issue tracker

A separate-towns game is a feature, not a fix, and it changes the simulation's data model. The
proposal is the phases above, each a small reviewable change, the first two already in:
`PlayerCommand` for the stock limits and notify flags (which also fixes a shared-town desync),
then the player's number through commands and recordings. Nothing about phases 3 to 10 changes a
single-player game or a shared-town one; each phase is checked by the comparison run (no save
differs) and by a two-town test.
