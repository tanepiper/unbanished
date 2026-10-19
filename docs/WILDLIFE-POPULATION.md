# Deer population resource handoff

This trace follows the `PopulationDeer` input selected by all three starting-condition profiles.
Evidence comes from the extracted `.crs` files and Ghidra 12.1.4 pseudocode for the Steam x64
executable. It is static analysis; no in-game population or wildlife count was observed.

## Map initialization handoff

In `FUN_1400a87d0`, the `MapGenerator` array at `+0x90` is iterated once per serialized entry. For
each path, the function clones the referenced internal with `FUN_1400c8bf0`, then calls
`FUN_1400c9d70` with that internal, the shared manager at `DAT_1401cbdd8 + 0xe8`, and the value at
`+0x20` of the loaded resource. The three shipped profiles each contain one `PopulationDeer` path.

`FUN_1400c9d70` attaches the cloned internal to the supplied manager, copies its resource key and
type value into internal fields, and invokes initialization callbacks. This establishes registration
of the population resource as manager-owned data. The function does not itself construct visible
deer or establish a starting herd count.

## Serialized resource links

The extracted files form this reference chain:

```text
MapGenerator +0x90
  -> Template/PopulationDeer.rsc:population
       -> Template/PopulationDeer.rsc:deerHerd
            -> Template/PopulationDeer.rsc:herd
                 -> Template/WildAnimalDeer.rsc:resource
```

The population resource includes the display string `Deer Population`. The `herd` record points to
the Deer wild-animal resource and contains additional numeric fields. The wild-animal resource in
turn references its model, character, animal, and wild-animal records. The animal record references
Venison and Leather raw-material resources.

Those links establish that the population definition is connected to a Deer herd and its entity and
resource templates. The serialized numeric values have not yet been mapped to field names or traced
through the population manager's update logic, so they are intentionally not labeled as population
size, spawn rate, distance, or inventory quantities.

## Open behavior questions

- Which manager update routine consumes registered population definitions, and when it runs.
- How the population record's numeric fields affect herd creation and replenishment.
- The initial number and positions of Deer entities in a new map.
- How Deer death or hunting updates Venison and Leather yields.
- Whether every profile uses identical wildlife initialization after registration.

## Port (`crates/game/src/nature/wildlife.rs`)

The population, herd and animal components are ported. Their scheduled updates run in the
entity scheduler's bucket pass (`Seeder::iterate_with`). The animals' CharacterComponent
moves them in the bit-0x04 phase (`Agents::move_animals`), and they are drawn like citizens
from their character.

| Piece | Original | Behaviour |
|---|---|---|
| PopulationDescription (`FUN_140055140`) | cell 8, herd block 64, habitat types 0x17, forbidden 0x68, occupants forbid, capacity 2, regrowth 1, recount on, herd `PopulationDeer.rsc:deerHerd`, name | |
| Setup (`FUN_140054b80`) | | Each population cell holds `[capacity, count, next]`. A cell's capacity is the description's when its 8 × 8 block has habitat and nothing forbidden (`FUN_1400544b0`). Then `(map / 64)²` herds start, one per block, each of `Rand(max − min) + min` deer (`FUN_140126420`), each registered after its deer |
| Population update (`FUN_1400549d0`) | | Every 300 s each count regrows to its capacity, then becomes the weighted mean of the habitable cells within 2, with weight `16 − dx² − dy²` (`FUN_140054220`). A sixteenth of the cells are then recounted |
| HerdDescription (`FUN_1401276b0`) | animal `WildAnimalDeer`, max 10, min 5, rest 1 + 0.75 (× 300 s), area 8, regrowth 12 (× 300 s) | |
| Herd target (`FUN_140127810`) | | A random habitable cell of the block (`FUN_140054600`); every deer keeps to the area around it (`FUN_140126fa0`). With no habitat left, the herd's deer are removed |
| Herd update (`FUN_1401270c0`) | | Every regrowth interval, deer join at the target up to `min(2 × count, max × share)`, where the share is the block's habitable fraction, doubled up to 1 (`FUN_140126920`). A resting herd waits out its rest, or moves on early if its target loses its habitat. A moving herd rests once all its deer are in their areas (`FUN_140126e50`) |
| AnimalDescription (`FUN_14010c520`) | Venison, Leather; area 8; life 3 ± 0.5 years; adult at 1; idle `+0x40` = 4 (constructor) | |
| Animal setup and activation | | Lifespan `RandFloatTol · 0.5 + 3` (`FUN_14010c220`). The home is where it stands, and its area is around it (`FUN_14010c0f0`). Then the update runs with no time, so a deer already past its lifespan dies at once |
| Animal update (`FUN_14010b8f0`) | | Age grows by `dt / 3600`, and past the lifespan the deer is removed. The model grows from a third of full size to full size by the adult age (`FUN_14010c7f0`). When idle and its timer has run out: a lost home (not ground, or holding an object) is replaced by the nearest free ground (`FUN_14010b4f0`). Out of its area, or 1 in 4 otherwise, it walks to random ground in the area (MoveTo walk 0, stand 1, mask 0xf9f); else it idles clip `Rand(4)` (or `Rand(3) + 1`) for `RandFloat · 4 + 4` s (`FUN_14010b720`) |

The herd's sex flag (`Rand(2) == 0`) is drawn once per spawn and gives males the variant's
second mesh. Seed 1234 Medium starts 64 herds with 460 deer. Blocks without habitat lose
their herd at its first update.

Not ported:
- livestock breeding (`+0x40` with flag `0x800000`; pastures);
- the trading post exception to a lost home (any object counts);
- the animals' animation audio events.
