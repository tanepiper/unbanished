# unbanished-terrain

A port of Banished's terrain: the map generator, the terrain mesh, the rivers, the ground height
queries and the cell types. The same seed and settings give the same map as the original, bit for
bit. Every stage was checked against output recorded from the original functions in the emulator,
and against the heights stored in real save files.

| Part | What it does |
|---|---|
| `Generator` | Makes the height grid from a seed and a size record, in five stages |
| `Generator::surface_map` | The packed per-cell surface image (dirt, rock, light) that the terrain shader samples |
| `TerrainMesh` | The mesh: 32 × 32-cell tiles, each with 33 × 33 vertices, sharing one index list |
| `HeightGrid` | Queries on the saved heights: ray casts, the ground height and normal, the natural cell type |
| `CellType` | The type of a map cell: ground, rough, high, shallow or deep water |
| `river_strip`, `water_mesh` | The triangle strip of a river, and the water surface |

## Use it

```toml
[dependencies]
unbanished-terrain = { path = "crates/terrain" }
unbanished-formats = { path = "crates/formats" }   # to read the terrain profiles
```

### Generate a map

A terrain profile (`Game/Terrain/Valleys.rsc` or `Game/Terrain/Mountains.rsc`) holds one size
record for each map size: small, medium and large. The generator takes a size record and a seed.

```rust
use unbanished_formats::game::terrain_generator;
use unbanished_formats::pkg::Archive;
use unbanished_terrain::{Generator, SizeRecord};

fn generate(archive: &Archive, seed: u32) -> unbanished_formats::Result<Generator> {
    let profile = terrain_generator(&archive.read_ref("Game/Terrain/Valleys.rsc:resource")?)?;
    let wire = profile.sizes[0]; // 0 small, 1 medium, 2 large
    let mut generator = Generator::new(SizeRecord::from_wire(wire), seed);
    generator.run(|stage, g| println!("stage {stage}: {} samples", g.grid.len()));
    Ok(generator)
}
```

After `run`, the generator holds:

- `size`: the map's size in cells (`2n`);
- `grid`: `(size + 1)²` float heights;
- `heights()`: the same heights as the game saves them, as `i16` (the float × 256);
- `paths`, `lakes`, `points`: the river paths and other records that the stages made.

`surface_map()` draws two more random numbers. Call it after `run`, as the game does.

### Ask the ground

```rust
use unbanished_terrain::{CellType, Generator, HeightGrid, TerrainMesh};

fn inspect(generator: &Generator) {
    let heights = generator.heights();
    let grid = HeightGrid { width: generator.width(), heights: &heights };

    // The ground height and normal at a map position (cells, centred on the map).
    let mut normal = [0.0; 3];
    let height = grid.ground(10.5, -3.25, &mut normal);

    // The natural type of every cell of the gameplay map (n × n cells in the middle of the
    // 2n-cell terrain), before anything is built.
    let n = generator.record.half_size() as usize;
    let water = (0..n * n)
        .filter(|&i| grid.natural_cell_type(n, i) == CellType::DEEP_WATER)
        .count();
    println!("height {height}, {water} cells of deep water");

    // The mesh that the renderer draws covers the whole terrain.
    let mesh = TerrainMesh::build(&grid, generator.size as usize);
    println!("{} vertices", mesh.vertices.len());
}
```

`CellType::blocks_movement` says whether citizens can walk on a cell. Shallow water, deep water
and high ground block movement.

## Coordinates

- The terrain of size `2n` has `2n + 1` height samples on each side. `n` is the first value of
  the size record (`SizeRecord::half_size`): 256, 384 or 512 cells for the small, medium and large maps.
- The playable gameplay map is the central `n × n` cells. The outer ring is scenery.
- Positions are in cells, relative to the centre of the terrain: x and y run from `-n` to `n`.
- A saved height is the generator's float height × 256. In world space, z (up) is the saved
  height divided by 256.

## Tests

The tests for this crate are in `crates/game/tests/`:

- `terrain_emu.rs` compares each generation stage, the cell types and the mesh with emulator
  output (make it with `tools/emu/terrain_stages.py`);
- `saves.rs` regenerates the terrain of real saves from their seed and settings and compares it
  with the stored heights.

Both skip when their data is missing.

```bash
cargo test --release -p unbanished-game --test terrain_emu --test saves
```

## The API

The full API documentation is at
<https://tanepiper.github.io/unbanished/api/unbanished_terrain/index.html>.

## Related documents

- [Terrain generation](https://tanepiper.github.io/unbanished/topics/terrain)
- [WORLD-GENERATION.md](https://github.com/tanepiper/unbanished/blob/main/docs/WORLD-GENERATION.md), [TERRAIN-PROFILES.md](https://github.com/tanepiper/unbanished/blob/main/docs/TERRAIN-PROFILES.md),
  [TERRAIN-DERIVED-GEOMETRY.md](https://github.com/tanepiper/unbanished/blob/main/docs/TERRAIN-DERIVED-GEOMETRY.md),
  [TERRAIN-WATER-GEOMETRY.md](https://github.com/tanepiper/unbanished/blob/main/docs/TERRAIN-WATER-GEOMETRY.md)
