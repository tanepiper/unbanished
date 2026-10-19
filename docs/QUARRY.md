# Quarry pit (QuarryComponent)

The quarry's pit deepens as the quarry is worked.

Port: `crates/game/src/production/quarry.rs`. The terrain change is applied with the other terraform
requests after the iteration (`StartingTown::apply_terraforms`).

## QuarryDescription (`Template\Quarry.rsc:quarry`, serializer `FUN_140055c80`)

| Field | Value | Meaning |
|---|---|---|
| `+0x2c` | −7 | the deep floor, reached at the end of production |
| `+0x28` | −1.4 | the shallow floor, when the quarry is built |
| `+0x20` | `Models\Buildings\Quarry\QuarryFloor.rsc` | the pit, a picking mesh in model space |

The ConsumeProduceDescription's lifetime (`+0x48`) is 2000 productions.

## Digging (`FUN_140055320`)

Each vertex of the pit mesh is placed by the model's transform. It sets the height of the grid
point under it, `(int)(x + 0.25 + half)` and `(int)(y + 0.25 + half)`, to the vertex's height,
no lower than the floor. The touched terrain tiles are rebuilt. The DecalComponent is removed
before the dig and added again after it, so its mesh follows the new ground.

## When it digs

- **Entity event 4** (the build's end, `FUN_14010cad0`; slot `+0x60`, `FUN_140055a50`). The pit
  is dug to the shallow floor.
- **Its message** (slot `+0x78`, `FUN_1400554e0`). Every batch the ConsumeProduce sequence
  produces (`FUN_1400c4440`) sends a message to the component whose type is the
  ConsumeProduceDescription's `+0x38`. The message runs after the worker puts the tool away and
  before the batch is created. The pit is dug to `(1 − t)·shallow + t·deep`, where
  `t = produced / lifetime`.
- **Claiming cells.** After the message's dig, every cell of the map rectangle is tested.
  The test takes the ground at four points, the cell's middle ± 0.25 on both axes, through
  `FUN_1400afef0`, a ray cast down from 200. A cell whose surface normal has `z < 0.85` at any
  of those points becomes type 5, with the quarry as its object (`+0x08`). The path grid and
  sectors follow.
