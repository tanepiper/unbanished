# Starting-map ambient audio emitters

This note follows the audio-related part of `MapGenerator` initialization in Ghidra 12.1.4 for the
Steam x64 executable and compares the serialized Hard, Medium, and Easy profiles. It describes
object placement calls, not audible runtime playback; no audio was played or observed in-game.

## Path-following River ambience

`FUN_1400a57c0` iterates the point sequences held at singleton `DAT_1401cbdd8 + 0x110`. It measures
the distance between successive 2D points and accumulates that distance for each sequence. When the
accumulated value exceeds the profile's `+0x130` threshold, the function clones
`Audio/Effects/RiverEffect.rsc:ambient`, finds its emitter child, moves that emitter to the current
path point `(x, y, 0)`, resets the accumulator to 0, and registers the clone through the shared
internal-object manager at `DAT_1401cbdd8 + 0xe8`. All three profiles set the threshold to `8.0`.

`+0x110` is the MapComponent and `+0x1e0` (count `+0x1e8`) its river path lists. At the end of
`FUN_1400b1980`, after `FUN_1400b0730` builds the water mesh, the list is resized to the
generator's path count (`+0x20`) and list `i` receives the 2D position (first 8 bytes) of every
28-byte record of generator path `i` (`+0x18`), last record first. The lists are saved with the
MapComponent (`FUN_1400b59a0`). Regenerating the seeds of two saves with the port's generator
gives exactly the saved lists, reversed, so the points are map coordinates (cells from the map
centre, `±n`). The port places the emitters from `World::river_paths`.

## Terrain-region Environment ambience

The same function walks the map in two dimensions, advancing both indices by the profile's
`+0x150` value (`16` for Hard, Medium, and Easy). For each region it clones
`Audio/Effects/Environment.rsc:ambient` and calls `FUN_140044000` with the region origin and width and
height. That helper stores the region coordinates, computes its center in the map grid, samples the
terrain with `FUN_1400a48b0`, assigns the resulting position to the emitter, and invokes the
component initialization path before the clone is registered with the shared manager.

This establishes one Environment ambient resource instance per visited 16-by-16 map region,
subject to the map-size loop bounds. It does not establish what sounds the resource plays or how
the audio engine chooses a currently audible emitter.

## Wind ambience

`FUN_1400a57c0` also clones one `Audio/Effects/WindEffect.rsc:ambient` resource and locates its
emitter child. It reads a vertical coordinate from the child's current position, adds the profile's
`+0x170` value, builds a position with zero horizontal coordinates, moves the emitter there, and
registers the instance through the same manager.

| Profile | `+0x170` vertical offset |
|---|---:|
| Hard | 20.0 |
| Medium | 30.0 |
| Easy | 40.0 |

The code establishes the offset used in the placed emitter position. It does not establish the
intended design reason for the difficulty-dependent values.

## Evidence and remaining questions

- MapGenerator serializer: `FUN_1400a7e50`.
- Profile consumer: `FUN_1400a57c0`.
- Terrain-region position setup: `FUN_140044000`.
- Emitter movement: `FUN_1400432c0` calls `Audio::Scene::MoveEmitter` when the position changes.
- The emitter resource definitions and audio-engine playback/update behavior remain to be traced.
- Confirmed: `+0x110 → +0x1e0` holds the generator's River paths (see above).
