# Separate towns: a town for each player on one map

Step 7 of [MULTIPLAYER-PLAN.md](MULTIPLAYER-PLAN.md). This page is the design and the order of
work; every phase below is done on the `fix/stock-limit-command` branch (fork).

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
| stock limits and warnings | disasters and tornadoes |
| the start cell, the camera, open windows, window settings | the event log and its notify flags, statistics, achievements |

The stock limit and the event log's notify flags are one per game (`Simulation::stock_warnings`,
`Simulation::events`). A separate towns game has one stock table per player; the event log stays
one, because every computer must post the same events in the same order to stay in step.

## The design

1. **Ownership.** `owner: u8` on `Placed` (one per placed entity) and on `Citizen`, 0 for the
   first player. The owner is set when the command that creates the entity runs, from
   `Simulation::acting_player`, and it is what every pick below filters on. A single-player and
   a shared-town game stamp 0 whatever the command's player: their one town's buildings are the
   town's.
2. **The mode is derived from the town.** `StartingTown::separate()` is true when the town has a
   start per player or anything in it belongs to a player past the first; `Simulation::separate()`
   reads it. A shared town is one town and one stock table whatever the number of players; only a
   separate towns game splits them. A save loaded without its owners sidecar therefore reads as a
   shared town — a game with every owner 0 — rather than failing.
3. **Who acts.** `Simulation::acting_player` is already carried by every command: the game loop
   knows the local player's number (`GameplayLoop::player`, 0 outside lockstep), `execute_from`
   hands another player's command on as theirs, `Watched::execute` passes the server's player,
   and a recording keeps each command's player. A command that places, builds, drafts a worker
   or sets a limit acts for that player.
4. **Per-player context.** The systems that hold per-player scalars in one place swap them per
   player while they run: the agents' manager at `Agents::sim`/`with_people`/`think`, and the
   stock manager. Each player's town is walked in player order, so the random draws stay in one
   order for every computer. What every computer must agree on is game-wide: the hooks see the
   first town's stock counts, the statistics see the towns added up, the event log gets every
   town's warnings, and the vanilla save writes the first town's stock block. A separate towns
   game's per-player limits and starts ride in the sidecar instead.
5. **One job market, filtered at the sort.** There is one `Jobs`; `Jobs::sort`
   (`crates/game/src/work/jobs/offers.rs:167`) is the single place a citizen's offers are
   chosen — it already filters them by the employing entity — so the player's owner goes there
   rather than in every caller. Same shape for stock: one storage list, filtered on the storage's
   owner (`registered_storages_of`).
6. **Save.** The vanilla save has no owner field and must not grow one (1:1 with the original's
   format). Ownership is written beside it, in `NAME.owners` next to `NAME.sav`: the magic
   `UNBO\x02`, the players, each placed entity's and citizen's player keyed by its object id in
   the save, each player's stock limits (`STOCK_SLOTS` each) and each player's start cell. The
   key is the object, not a position, because a save drops what died and moves the last entity
   into the free place: a position in the town's list and in the file name different things. A
   save read without a sidecar is a game with every owner 0.
7. **Digest.** Every player's save must hash the same, so ownership is in the digest and the
   camera is not, as today. A separate towns game hashes the owners' bytes after the state's
   CRC; a single-player game's digest is its state's CRC alone, unchanged.
8. **The lobby.** `Message::Start` says which mode the room is (`separate: bool`) and carries the
   owners. Every computer must run the save's own mode; a room whose mode and save disagree, or
   whose save has not a town for every player, is refused with the reason instead of desyncing.

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
Rendering draws every player's entities; picking with the mouse takes only the local player's
(`GameplayLoop::may_select`), so another player's town is seen but not selected or opened.

## A new game, and how a room plays one

`StartingTown::create_for` takes a player count: player 0's start is the original's search from
the map centre (`FUN_1400a7bf0`), each next one `Map::find_start_away` four start radii from the
ones taken, and every player gets the profile's structures, a household and its own starting goods
in its own storages there. `StartingTown::create` is that with one player, which draws the same
random numbers in the same order, so a single-player game is unchanged. The camera opens over
the local player's start.

A host makes such a save with `UNBANISHED_SEPARATE_TOWNS=N` set (`=2` for two towns) and then
starts a New Game; the save is written with its sidecar. The host then opens the room from that
save as any other. Every player needs the same build, data and mods, as in a shared town game.

## Phases and how each one is checked

| Phase | Work | Check |
|---|---|---|
| 1 | A resource limit and an event-log notification are `PlayerCommand`s | done: `crates/sim/tests/setting_commands.rs`, quick tier |
| 2 | Every command says whose it is, through the game loop, lockstep, the server and recordings | done: `apps/game/src/gameplay/tests/commands.rs`, `crates/session/src/recording.rs`; a new-game recording made by the build before it has the same entries and replays identically |
| 3 | `owner` on `Placed` and `Citizen`, set from `acting_player` | done: a two-owner town, owners kept through a save; single-player owners all 0 |
| 4 | Stock per owner: `materials`, `storages`, the stock manager, limits and warnings | done: two towns' stock never mix; a player's limit does not apply to the other's; one table in a shared town |
| 5 | Jobs per owner: `Jobs::sort`, professions, workplaces | done: a citizen takes only its owner's offers; the lockstep test's digests agree |
| 6 | Citizens and agents per owner: `Env` carries each citizen's owner and every player's stock, so a workplace's rules read its own player's table (`Sim::stock_of`, `stock_of_citizen`) | done: the two-town lockstep games agree on every digest, so the shared `Random` draws in one order; a citizen is offered only its own town's work |
| 7 | Drawing and picking: every owner's entities drawn, only the local player's picked | done: `GameplayLoop::may_select`, `apps/game/src/gameplay/tests/towns.rs`; the pick filter is a no-op with one town |
| 8 | Save and resync: ownership written beside the save, sent with `Message::Save` | done: a save of a two-town game reloads with the same owners; a resync agrees |
| 9 | The lobby: the mode in `Message::Start`, refused by a build without it | done: `separate` in `Message::Start`; `a_room_refuses_a_save_of_the_other_mode` checks both refusals and the matching start; `tools/compat/multiplayer.sh towns` plays a two-town room over a relay |
| 10 | The end-to-end test: a room of two separate towns over a simulated network | done: `tools/compat/lockstep.sh`'s separate-towns block runs both settings: every digest agrees, and the corrupted player is found and resynced |

Phases 3 to 6 are one change in spirit — the data model and its filters — and each ends with
the single-player game drawing and saving the same: `StartingTown::create` is `create_for` with
one player, which draws the same random numbers in the same order. `tools/compat/compare.sh
ab22835 HEAD new-game` agrees at every step it checks: the base build against itself, the branch
against the base, and both continuing the base's save, with identical game state and frame.
`tools/triage/determinism.sh` finds the frames identical at 16 s. The probe's new-game scenario
matches its baseline except the `tools` count difference main already had. Only the `new-game`
scenario ran: the others need the project's save set, which the gaming PC does not have.

## What a two-town game measured

On 2026-11-17, on the gaming PC (Linux, release build):

- A two-town New Game's save writes `NAME.owners` of 37,021 bytes for 7,324 placed entities, 39
  citizens and 2 players (each entry is the object's id and its owner).
- `tools/compat/lockstep.sh`'s separate-towns block ran both of its settings through the
  script: with delay 6, jitter 8 and 20% stalled ticks no desync was found and the players agreed
  on 11 digests; with player 1's `Random` corrupted at update 400 it was found at 420 and
  resynced, then agreed to the end. (The script's shared-town settings still need its save set.)
- The same settings on a single-town save played by two players (a shared town) agreed as before.
- `tools/compat/multiplayer.sh towns` played a two-town room over a relay: the guest joined and
  started from the host's two-town save, and the host's 27 checks all agreed with none differing.
- The two-town save run as one game peaks at 1.59 GB resident memory for 600 updates (7.8 s,
  native release, no drawing); the lockstep test, which runs two complete games in one process,
  peaks at 1.65 GB. The browser build's heap was not measured.
- `crate::sim`'s unit tests, the net, session and server tests, and the app's unit suite pass,
  with the same environment failures as on main: three audio tests and the burning buildings'
  particles in the app, and the save-dependent bucket test in `unbanished-sim`.

## Remaining gaps

- **The event log and its notify flags are one per game.** A separate towns player's notify
  checkbox turns off a group for everybody; a per-player log would need one log per player, all
  posted in the same order, or a per-player filter at the window. *Open.*
- **Statistics are the towns added up.** The Town Info graphs show the game's total, not the
  local player's town. A per-player graph must still hash the same on every computer (the digest
  does not read them), so this is only an interface change. *Open.*
- **Achievements are game-wide**, as in a shared town. *Assumed* acceptable.
- **Trade between towns** is the shared-town merchants; players cannot trade with each other.
- **Floors and clearing are shared**, so every player sees every player's roads and cleared land.
- **The browser has no load path**, so it neither reads nor writes a sidecar; a separate towns
  game is native-only until the browser's save store gains one.

## Open questions

- **Trade between towns.** The shared-town trade is with travelling merchants; between players
  nothing exists yet. Out of scope for the phases above.
- **Memory.** N towns is N citizen lists in one process. A two-town game measures 1.59 GB
  resident natively (the lockstep test with two complete games, 1.65 GB), within the browser's
  4 GB heap but its own build was not measured.
- **Floors and clearing.** Each player's floors are on the shared cells, and each player's
  buildings are stamped on them. *Assumed*: shared, as the mod's map is one map.
- **Wildlife and disasters.** Shared: whoever hunts takes from the same herd, and a tornado
  damages every player's buildings where it passes. *Assumed*.

## For the issue tracker

A separate-towns game is a feature, not a fix, and it changes the simulation's data model. All
ten phases are implemented on `fix/stock-limit-command`: `PlayerCommand` for the stock limits and
notify flags (which also fixes a shared-town desync), the player's number through commands and
recordings, ownership on every entity and citizen, per-player stock and jobs, per-player starts
and a sidecar for saves and resyncs, the mode in the lobby, and the end-to-end lockstep test.
Nothing about phases 3 to 10 changes a single-player game or a shared-town one; each phase is
checked by the comparison run (no save differs) and by a two-town test. The gaps above are the
follow-up work.
