# The commute optimisation

Every 300 s of game time the town rearranges where people live and work, so that families
live near their jobs and jobs go to the people living nearest. It runs from StockManager::Update
(`FUN_140091780`). Port: `crates/game/src/commute.rs`, called from the gameplay iteration right
after the stock recount. Test: `the_commute_run_keeps_the_professions`.

## Clock and job

The StockManager keeps the time since the last run at `+0x190` and a job object at `+0x1b8`:
the cell pairs (`+0x00` array, `+0x08` count), the next pair's index (`+0x10`, −1 idle) and a
region-pair → rank map (`+0x18`). Each update:

1. `+0x190` grows by the game time.
2. From 300 s on, `FUN_1400dc3d0` takes a step. While the job is idle it returns false and
   `FUN_1400dc590` starts one.
3. Once a step has ranked the last pair, the clock goes back by 300 s at a time until it is at
   most 300 s.

With no pairs to rank (no workplaces or residences in regions), the start runs again every
update, so the homes and jobs passes never come.

## Start (`FUN_1400dc590`, only while idle)

- **Residence entrances.** One for each residence in the town's list (`+0x20`), taken with
  `FUN_14008d480(.., 0)`. Its `Rand(count)` draw comes with several use points.
- **The rank map is cleared.**
- **Pairs.** For each profession in list order, and each of its workplaces in the registry's
  current (shuffled) order:
  - take the workplace's entrance (another draw);
  - pair it with each residence entrance;
  - skip the pair if either cell has no region (`+0x18`), or the region pair is already in the
    map;
  - otherwise append the pair `(max cell, min cell)` and enter the region pair
    `(max region, min region)` with rank 0.
- **Index.** The index is set to the pair count − 1.

## Step (`FUN_1400dc3d0`)

- **Ranking.** Up to `min(index, 6) + 1` pairs, last first, each:
  - its rank is `Map::PathRank(a, b, 0)`, which stores nothing;
  - 0 or 1 (an unsigned test, so −1 stays) become `(int)sqrtf(dy² + dx²)` in cells;
  - the rank goes into the map under the cells' region pair.
- **Finishing.** When the index passes 0, the homes pass runs, then the jobs pass, and the pairs
  are cleared.

## Homes (`FUN_1400db770`)

**Which residences.** Only those with fewer than two family slots (`+0x58`); a hostel is left
alone. An empty one (slot 0 has no one) is a place to move to. An occupied one gets its cost.

**The cost** (`FUN_1400dc130(family, home)`). Start from the home's use point
(`Interact::PointPosition`, clamped to the map). For each member of the family in slot 0 that
is:
- an adult (CitizenDescription `+0x44`);
- not at school;
- with a workplace;
- not a laborer,

take the workplace's entrance (a `FUN_14008d480` draw). When `Map::Reachable(0xe1f)` from the
home, add `FUN_140008830` (`PathRank` mode 1, else map width plus the straight distance).

**The move.** With any empty residence, the occupied ones go in order of cost, highest first
(equal costs by residence). For each, every empty residence in turn (by residence) prices the
same family from its point. The first strictly cheaper than the best so far wins. If one wins:
- every member of slot 0 moves there (`FUN_14011b8a0(c, home, 0)`, dependents with them). The
  loop runs over the live list, as the original's does.
- each stored item whose unreserved count fits whole in the new home's storage
  (`FUN_140132250`) is added there (`FUN_1401324d0`) and taken from the old one
  (`FUN_140132620`).
- the new home is no longer empty; with no empty ones left the pass ends.

## Jobs (`FUN_1400d9880`)

The pass runs in six steps.

1. **Mismatches.** Every working adult whose workplace takes another profession (WorkPlace
   `+0x48`) becomes a laborer without a workplace (`FUN_1401173b0`).
2. **Workplaces.** For each profession in list order and each of its workplaces in registry
   order, a workplace with workers (`+0x60`) becomes a record: its entrance (a draw), places
   left = total = its worker count, and an empty candidate set.
3. **Candidates.** For each adult, not at school, in citizen order:
   - it joins the free set;
   - the mismatch check runs again;
   - its profession is counted in the quota;
   - it is made a laborer without a workplace;
   - from its home's use point (−1 without a home), it is scored against every record
     (`FUN_1400db530`) and entered as `(score, citizen)`.
4. **The queue.** Each record goes in at its best (lowest) score times
   `1 − (total − left) / total`, truncated (f32 maths).
5. **Dealing.** While the queue has records:
   - take the highest priority (ties: the first record);
   - its best candidate takes the workplace and its profession;
   - the profession's quota drops, and the citizen leaves the free set;
   - the record's places left drop.
   - **Updating the others.** For every record with places left (this one included), the
     citizen is scored again and its entry found (by key, else by search). If it was that
     record's best, the record comes out of the queue (at the priority it was entered with)
     and goes back in at its new best. Either way the entry goes.
6. **Leftovers.** While quota is left, the first free citizen takes the first profession left
   (profession order), without a workplace.

So every workplace keeps its number of workers and every profession its number of people. Who
holds which changes, builders and laborers included.

**The score** (`FUN_1400db530`):
- `0x1000000` without a home or a walkable way (`Map::Reachable(0xe1f)`);
- with a cell outside every region, `(int)sqrtf(dy² + dx²)`;
- else the rank map's value for the cells' region pair. A pair not yet there is ranked now
  (`FUN_140008830`; 0 or 1 become the straight distance) and entered, but this score is 0. The
  register is zeroed and never loaded from the new entry. Negative ranks count `0x1000000`.

## Deviations

- The trees order their citizens, residences and professions by object address; the port uses
  their indices (creation order), which is the usual allocation order.
- The citizens are taken in index order. The original walks the town's citizen list
  (`+0xa0`), which is the same order unless a citizen was removed and an index reused.
- The state lives in `Construction::commute`; it is not yet saved.
