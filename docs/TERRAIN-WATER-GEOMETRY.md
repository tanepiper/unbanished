# River and lake geometry flow

This note connects the terrain generator's two distinct geometry inputs to their render materials.
Evidence is static Ghidra 12.1.4 pseudocode for the Steam x64 executable. It establishes the
engine's data and draw bindings, not a visual runtime capture.

## River path records

`FUN_1400d1d20` appends 28-byte path entries to the generation context's counted collection at
`+0x18` (count at `+0x20`). It builds candidate paths from endpoints and recursively displaced
midpoints, rejecting candidates that intersect existing segments. `FUN_1400d2560` ensures the list
is nonempty, and `FUN_1400d2430` tries the configured number of additional paths.

During mesh construction, `FUN_1400b0730` iterates that `+0x18` path collection and calls
`FUN_1400d4fd0` for each path. It appends each result to a second vertex/index mesh and stores per
path draw ranges in the list at MapComponent `+0x1c0` (count at `+0x1c8`). The MapComponent render
method `FUN_1400b3960` submits that list with the selected `River.rsc` material at `+0x150`.
`FUN_1400b3ee0` handles the corresponding draw flag by binding the path mesh buffers at `+0x160`
and `+0x180` and selecting the draw range from `+0x1c0`.

This producer-to-material chain establishes that the `+0x18` generated path records drive geometry
drawn with the River material.

## Lake point records

`FUN_1400d2620` also creates separate 12-byte records and appends the lake-surface input records to
the generation context's collection at `+0x48` (count at `+0x50`). `FUN_1400b0730` iterates those
records separately. It reads two coordinates and a third scalar; that scalar expands the bounded
grid region examined around the point. It samples the generated height grid and builds vertices and
indices for cells that satisfy its height tests, then stores the result as per-lake draw ranges in
MapComponent `+0x1d0` (count at `+0x1d8`).

`FUN_1400b3960` submits this second list with the selected `Lake.rsc` material at `+0x158`.
`FUN_1400b3ee0` binds the same path-mesh vertex/index buffers but selects lake draw ranges from
`+0x1d0` under a separate draw flag. The profile's Lake material instance points to
`LakeMaterial.rsc`; its texture-slot links are listed in
[TERRAIN-MATERIAL-RESOURCES.md](TERRAIN-MATERIAL-RESOURCES.md).

This establishes a separate point-and-height-grid path from the `+0x48` records to geometry drawn
with the Lake material. It is distinct from the path-record collection used for River draws.

## Render condition and scope

The render method queues both water lists only when the current renderer shader value is not `1`.
The terrain tile list uses the terrain material at `+0xf8` and its own buffer; the river and lake
lists use the secondary mesh buffers and separate draw-range metadata. Resource names, producer
collections, material pointers, and draw submission now agree on the River/Lake split.

This remains static evidence. It does not establish water-flow simulation, navigation blocking,
shoreline rules, or the in-game visual appearance of generated surfaces.
