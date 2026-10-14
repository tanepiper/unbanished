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
