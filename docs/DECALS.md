# Terrain decals

The map draws two decal lists over the terrain:

- **Entity decals** (the map's `+0x1f0`, 0x60-byte entries), added by DecalComponents. These
  cover building footprints, fields, yards, pastures, the cemetery, tunnels, bridges and clear
  markers.
- **Road decals** (`+0x200`, per 32-cell tile and road kind). See [ROADS.md](ROADS.md).

The port draws the entity decals first, then the roads. Both are rebuilt when anything they
are built from changes: the road revision, the terrain revision, or the entities' rectangle,
rotation, variant and decal state.

## DecalDescription (`Template\X.rsc:decal`, serializer `FUN_140120e50`)

The serializer writes, in order:

| Field | Meaning |
|---|---|
| `+0x20` f32 | Alpha until the entity is built: 0.5 for most, 1 for pastures, the cemetery, clear markers and the storage cart. |
| `+0x28` | Materials (external references). Buildings have a footprint material plus a damaged one. |
| `+0x48` f32 | Texture scale: 2048 for footprints, 1024 for the quarry, 1 when tiled. |
| `+0x38` | One texture rectangle (`u0, v0, u1, v1`) per model variant. Houses have six, tiled decals none. |
| `+0x24` bool | Tiled. |
| `+0x25` bool | Set only by `Clear`. |

Thirty-three templates have one. `formats::game::decal_description` parses all of them, and
`crates/formats/tests/templates.rs` checks this.

## DecalComponent (vtable `0x140169400`, 0x38 bytes)

| Slot | Function | What it does |
|---|---|---|
| Setup | `FUN_140120ca0` | Copies the description's alpha into `+0x30`. |
| Activate | `FUN_140120a30` | Calls `FUN_1400b6250(map, x, y, x + w − 1, y + h − 1, alpha × 255, MapComponent +0x30 rotation, materials[+0x28], tiled, scale, rects[ModelComponent +0x164])`. The default rectangle is `0, 0, 1, 1`. The entry index goes to `+0x2c`. |
| Remove | `FUN_1401209e0` | Calls `FUN_1400b61d0`. |
| Event 1 | `FUN_140120c30` | Sets the alpha to 1 and re-adds the decal. |
| Event 5 | `FUN_140120c30` | Re-adds the decal. |
| Serialize | `FUN_140120cf0` | Writes `+0x30` alpha, then `+0x28` material from version 1. |

These change the alpha or material:

- **Opaque at state 8.** The build's state 8 (`FUN_14010cfb0`) makes the decal opaque.
- **Opaque on entity event 1.** The ClearArea's state 2 sends entity event 1 (`FUN_14011ccc0`), which also makes the decal opaque. Starting buildings reach state 2 this way.
- **Damaged material on collapse.** A collapse (`FUN_140122aa0`) switches to the DestroyDescription's `+0x2c` material.
- **Quarry re-adds.** The quarry re-adds its decal around its terrain dig (`FUN_140055320`).

Port: `banished_game::work::construction::Decal` (`Construction::decals`, by entity) holds the
opaque flag and the material.

## Meshing (`FUN_1400b2310`)

The decal gets a quad per cell of the rectangle. Its corners come from `FUN_1400b20e0`, the
same 20-byte vertex as the roads use, with the alpha byte. The diagonal alternates with cell
parity. The original de-duplicates vertices; the port does not.

The atlas uv (bytes 12 to 16) depends on the kind of decal:

- **Tiled:** the road atlas tile chosen by which neighbours are inside the rectangle (`FUN_1400b3650`).
- **Picture:** the full tile, `DAT_1401ca450[255]` = 46.

The decal uv (bytes 8 to 12) is `Transform2` applied to the corner's position in the
rectangle, `p ∈ [0, 1]²`. `Math::Transform2` uses row vectors, and `A * B` applies `A`
first. The chain is:

```
place · T(−c) · Rotate(yaw) · T(+c) · rect · Scale(s)
```

| Term | Picture | Tiled |
|---|---|---|
| place | `(x, 1 − y)` | `(x·w + x0, y·h + y0)` |
| c | 0.5 | 0 |
| rect | `(u0 + x(u1 − u0), v0 + y(v1 − v0)) / scale` | identity |
| s | 4096 | 1 |

Rotate has rows `(cos, sin)` and `(−sin, cos)`, with the angle in degrees.

The shaders sample the footprint at uv / 4096 and the tiled texture at uv / 16.

The menu scenes' serialized `+0x1f0` decals were meshed by the original. Rebuilding every
one of them, 2,270 in all, from its own entry gives the same atlas uvs and decal uvs within
one unit. The test is `building_decals::scene_check`.

Port: `apps/game/src/gfx/building_decals.rs`.

Not ported: the radius filter (`FUN_1400b2310`'s `param_3..5`), which no DecalComponent uses.
