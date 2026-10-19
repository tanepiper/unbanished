# banished-path

A port of Banished's path finding. It finds the cells a citizen or an animal walks along, and the
"path rank" distance that the game uses to choose the nearest storage, job or resource. Both are
tested against output recorded from the original functions in the emulator.

| Module | What it contains | Original |
|---|---|---|
| `pathfind` | Walkable regions, the A* pathfinder over the cell grid, request checks, cost tables | `FUN_1400a1950`, `FUN_14001d760`, `FUN_1400a2480`, `FUN_1400a3a20` |
| `sectors` | The sector graph: the map cut into 8 × 8-cell sectors, linked areas, and cached distances | `FUN_1400a4ce0` and its helpers |

`banished-game` re-exports both modules as `banished_game::pathfind` and
`banished_game::sectors`.

## Use it

```toml
[dependencies]
banished-path = { path = "crates/path" }
banished-terrain = { path = "crates/terrain" }   # for CellType
```

Cells are numbered row by row: cell `y * size + x` on a map of `size × size` cells. Each cell has
a `CellType` from `banished-terrain`.

### Find a path

```rust
use banished_path::pathfind::{regions, request, Pathfinder, WALKABLE};
use banished_terrain::CellType;

fn walk(size: usize, types: &[CellType], from: i32, to: i32) -> Option<Vec<i32>> {
    // Label the connected walkable areas. A request between two areas fails at once.
    let region = regions(size, types);
    let mut finder = Pathfinder::new(size, types);
    // The cells from `from` to `to`, both included, or None.
    request(&mut finder, types, &region, WALKABLE, from, to)
}
```

- `regions` gives every cell the number of its connected walkable area (0 for none). The original
  keeps it in the map cell at `+0x26`.
- `Pathfinder::new` starts with diagonal steps on, as the original does. `set_mode` chooses a
  mover's cost table, `set_type` and `set_blocked` change one cell when the map changes.
- `request` checks the request as the original does: if neither end is water, both ends must be
  in the same region. Then it searches.
- `nearest_neighbour` finds the cell next to a cell that is nearest to a target, among the cells
  whose type is allowed.

The original solves paths on a worker thread, and walkers poll for the result. The port solves a
path when it is requested; the walker still reads the result on its next update, so the timing
is the same.

### Rank distances

```rust
use banished_path::sectors::SectorGraph;
use banished_terrain::CellType;

fn rank(size: usize, types: &[CellType], a: i32, b: i32) -> i32 {
    let mut graph = SectorGraph::new(size, types);
    // -1 when b cannot be reached from a. `store` caches the result for 120 s of game time.
    graph.distance(a, b, true, true, types)
}
```

When the map changes, `mark(x, y)` marks the sector of a cell, and `rebuild(types)` rebuilds the
marked sectors before the next search. `age_cache(dt)` ages the cached distances.

## Tests

```bash
cargo test --release -p banished-path
cargo test --release -p banished-game --test astar_emu --test regions_emu --test sectors_emu
```

The `*_emu` tests compare with output from `tools/emu/astar.py`, `regions.py` and `sectors.py`,
and skip when it is missing.

## The API

The full API documentation is at
<https://tanepiper.github.io/unbanished/api/banished_path/index.html>.

## Related documents

- [Citizens](../../docs/topics/CITIZENS.md)
- [CITIZEN-AI.md](../../docs/CITIZEN-AI.md): the decisions, the action queue and walking
