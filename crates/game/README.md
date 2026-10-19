# banished-game

The rules of Banished 1.0.7, ported to Rust: citizens and their lives, jobs and professions,
construction, production, farming, trade, wildlife, the weather, and disasters. Each ported
function names the original function it came from, keeps the original's order of arithmetic and
its float precision, and draws random numbers in the same order. Tests compare the results with
output from the original functions in the emulator, and with real save files.

The crate has no graphics, windows or sound, and it is deterministic: the same inputs and the
same seed always give the same results. The game program (`apps/game`) owns the town's state
and calls these rules every simulation step.

## Modules

The modules are grouped by domain. Each folder's `mod.rs` says what it holds.

| Folder | What it contains |
|---|---|
| `people` | Citizens: ages and lives, households, names, needs, health, happiness, services, status counters, immigration, burial |
| `behaviour` | How characters act: the citizen AI and its activities, the agents that run citizens and animals, action stacks, animation, commuting |
| `work` | The work economy: the job manager, professions, construction and demolition, production recipes, storage, building controls, roads |
| `production` | Workplaces that gather or grow resources: quarries and mines, foresters, herbalists, hunters, fishing docks, crop fields, orchards, pastures |
| `commerce` | Markets, the trading post and the trader boats |
| `nature` | Resource seeding and spawning, growth, wildlife, and the environment (temperature and weather) |
| `world` | The gameplay map and its cells, bridges, building placement, terraforming |
| `disasters` | Fire, crop blight, tornadoes |
| `town.rs` | The simulation context that jobs and construction run in (`Sim`) |
| `events.rs` | The event log and stock warnings |

It also re-exports the crates it builds on, so that one dependency is enough:

| Path | Is the same as |
|---|---|
| `banished_game::{calendar, crt, math, noise, picking, random, scheduler}` | the modules of `banished-core` |
| `banished_game::terrain` | the `banished-terrain` crate |
| `banished_game::{pathfind, sectors}` | the modules of `banished-path` |

## Use it

```toml
[dependencies]
banished-game = { path = "crates/game" }
banished-formats = { path = "crates/formats" }
```

### Name a citizen

Names come from the game's name databases (`Game/Names/Male`, `Female` and `Cities`). The same
seed gives the same names as the original.

```rust
use banished_formats::names::NameDatabase;
use banished_formats::pkg::Archive;
use banished_game::people::names::generate;
use banished_game::random::Random;

fn three_names(archive: &Archive) -> banished_formats::Result<Vec<String>> {
    let db = NameDatabase::read(&archive.read_ref("Game/Names/Male.rsc:resource")?)?;
    let mut rng = Random::new(7);
    // A first name has 3 to 10 letters; a town name has 5 to 20.
    Ok((0..3).map(|_| generate(&db, &mut rng, 3, 10)).collect())
}
```

### Find the start of a new town

`world::map::Map` classifies every cell of a generated terrain. `find_start` finds the town's
start cell as the original does: in rings outward from the centre of the map, the first place
where every cell within `radius` is buildable ground. The start condition profile gives the
radius (`map_generator(...).start_radius`).

```rust
use banished_game::terrain::{Generator, HeightGrid};
use banished_game::world::map::Map;

fn start_cell(generator: &Generator, radius: i32) -> (i32, i32) {
    let heights = generator.heights();
    let terrain = HeightGrid { width: generator.width(), heights: &heights };
    let map = Map::new(generator.size as usize, &terrain);
    map.find_start(radius)
}
```

[banished-terrain](../terrain/README.md) shows how to make the `Generator`.

### More

The rest of the crate is the simulation itself. The best way to learn it is to read the topic
documents, then the module that ports the part you are interested in:

- [Citizens](../../docs/topics/CITIZENS.md): decisions, actions and walking;
- [JOBS.md](../../docs/JOBS.md): construction, work and professions;
- [CITIZEN-LIFE.md](../../docs/CITIZEN-LIFE.md): ageing, households and needs;
- [FARMING.md](../../docs/FARMING.md), [FIRE.md](../../docs/FIRE.md), [TRADE.md](../../docs/TRADE.md),
  [WILDLIFE-POPULATION.md](../../docs/WILDLIFE-POPULATION.md) and the other notes in `docs/`.

## Tests

```bash
cargo test --release -p banished-game
```

The unit tests run without game data. The tests in `tests/` compare with emulator output or with
real saves. Each one names the generator in `tools/emu/` that makes its data, and skips when the
data is missing. For example:

| Test | What it checks |
|---|---|
| `terrain_emu` | Each terrain generation stage, the cell types, the mesh and the start point |
| `saves` | The terrain of real saves, regenerated from their seed and settings |
| `names_emu` | Citizen and town names |
| `astar_emu`, `regions_emu`, `sectors_emu` | Path finding |
| `game_loop_emu`, `time_emu` | The simulation step and game time |
| `growth_emu`, `storage_emu`, `equipment_emu` | Growth, storage and equipment |
| `crt_math`, `transform`, `picking_emu` | The exact maths of `banished-core` |

## Example programs

`examples/` has helpers for animation work: `clips` (the clips of an animation group),
`pose_bones`, `pose_scan` and `skin_bounds`. `rng_trace_names` records the random draws of name
generation, for comparison with the original (see
[Emulate for exact numbers](../../docs/develop/guides/EMULATE.md#random-draw-parity)).

## The API

The full API documentation is at
<https://tanepiper.github.io/unbanished/api/banished_game/index.html>.
