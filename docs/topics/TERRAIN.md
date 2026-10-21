# Terrain generation

In Banished, the map comes from a seed. The same seed and the same settings always give the
same map. Un-Banished makes the same maps, bit for bit. This page explains the steps. The code is
in [unbanished-terrain](../../crates/terrain/README.md).

## The inputs

The New Game dialog chooses three profiles and a seed:

| Setting | Resource | Class |
|---|---|---|
| Terrain type (Valleys, Mountains) | `Game/Terrain/*.rsc` | `TerrainGenerator` |
| Climate (Mild, Fair, Harsh) | `Game/Climate/*.rsc` | `ClimateGenerator` |
| Starting conditions (Easy, Medium, Hard) | `Game/StartCondition/*.rsc` | `MapGenerator` |

A terrain profile holds one *size record* for each map size. Its first value, `n`, is 256, 384
or 512. The terrain is `2n` cells wide, so it has `2n + 1` height samples on each side. The
playable map is the central `n × n` cells; the outer ring is scenery.

The game's random number generator is seeded with the map seed. Every step below draws from it,
in a fixed order.

## The five stages

The generator runs five stages on a float height grid:

| Stage | What it does |
|---|---|
| 1 | Fills the grid from two Perlin noise fields, each with its own random seeds. |
| 2 | Makes the river paths (random end points, with midpoints displaced again and again) and the lake points. A path that crosses itself or another path is rejected. |
| 3 | Adds random local changes, then draws the paths and points into extra grids and combines them with the heights. (*Inferred*: this shapes the river beds and the lakes.) |
| 4 | Walks from random cells downhill and moves part of the height difference (*inferred*: a simple erosion). |
| 5 | Applies the paths and points once more, then smooths the grid twice. |

Then the float heights are multiplied by 256 and stored as 16-bit integers. These are the heights
that a save file stores.

## After the heights

- **The surface image**: one 32-bit value per cell, which packs how much dirt,
  rock and light the terrain shader uses there. It draws two more random numbers for its own
  Perlin noise.
- **The mesh**: the terrain is cut into tiles of 32 × 32 cells. Each tile has
  33 × 33 vertices, and the diagonal of each cell alternates by position, like a chequerboard.
- **The water**: the river paths become a triangle strip drawn with the River material
  and the lake points become the lake surface.

  <video src="../assets/videos/river-flight.webm" poster="../assets/videos/river-flight-poster.webp" autoplay loop muted playsinline width="720"></video>
- **The cell types**: the game casts a ray down at the centre of each map cell
  and reads the ground height and slope. Deep water is below -1.4, shallow water below -0.25,
  buildable ground is between -0.25 and 0.75 on flat ground, rough ground is below 2.0, and high
  ground is the rest. Water and high ground block walking.
- **The start point**: the game searches in rings outward from the centre for
  the first place where all cells within the start radius are buildable ground.

## How it was proved

The terrain was the first large part ported. It set the method for the rest:

1. The original's generator was run in the emulator with many seeds and sizes.
   `tools/emu/terrain_stages.py` wrote the grid after each of the five stages.
2. The Rust port was compared stage by stage. When a stage differed, the first differing cell
   showed where to look in the disassembly.
3. Then real saves were used: the port regenerates the terrain from a save's seed and settings,
   and compares it with the heights that the original stored in the save.

The tests `terrain_emu` and `saves` in `crates/game/tests/` do this. The cell types match 99.95 %
of the free cells of a large save; the cells that differ are where buildings changed the ground.

## Read more

- [WORLD-GENERATION.md](../WORLD-GENERATION.md): from the New Game dialog to the generator.
- [TERRAIN-GRID-TRANSFORMS.md](../TERRAIN-GRID-TRANSFORMS.md): the five stages in detail.
- [TERRAIN-PROFILES.md](../TERRAIN-PROFILES.md): the values in each profile and size.
- [TERRAIN-DERIVED-GEOMETRY.md](../TERRAIN-DERIVED-GEOMETRY.md) and
  [TERRAIN-WATER-GEOMETRY.md](../TERRAIN-WATER-GEOMETRY.md): the mesh, the surface image and the
  water.
- [MAP-CELLS.md](../MAP-CELLS.md): the gameplay map, the ground query and the cell types.
- [START-CONDITION-PROFILES.md](../START-CONDITION-PROFILES.md): what a start condition places.
