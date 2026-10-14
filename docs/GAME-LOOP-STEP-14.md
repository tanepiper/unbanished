# Game loop step 14: resource-description transition families

This step correlates the `NaturalResourceDescription` transition flag used by
[`GAME-LOOP-STEP-13.md`](GAME-LOOP-STEP-13.md) with the shipped compiled resources. The values were
read from the `data0.pkg` entries using the documented `NaturalResourceDescription` serializer
order in `FUN_14012b2e0`. This is template evidence, not a live transition capture; modded or
overridden resources may differ.

## Description flag and dispatch

`NaturalResourceDescription` byte `+0x64` selects the two branches in `FUN_14012b910`:

- Set: request mesh animation index `2` through `FUN_14012a5c0`.
- Clear: enter `FUN_14012b5c0`, which queues the current owner for removal when registered and can
  create/register a referenced-resource entity if `NaturalResourceComponent+0x2d` is set.

All 24 `NaturalResourceDescription` entries in the shipped `data0.pkg` were decoded through this
field. The nine set values are all tree resources; each references the wood raw-material resource.
The other fifteen are not set:

| Animation-index-2 branch | Resources | Referenced raw material |
|---|---|---|
| Set | AppleTree, CherryTree, ChestnutTree, PeachTree, PearTree, PecanTree, PlumTree, Tree, WalnutTree | `Template\\RawMaterialWood.rsc:resource` |
| Clear | Bean, Blueberry, Cabbage, Corn, Herb, Mushroom, Onion, Ore, Pepper, Potato, Pumpkin, Rock, Root, Squash, Wheat | Resource-specific raw material |

This establishes that the shipped tree family selects the animation branch and all other listed
natural-resource families select the alternate branch. It does not establish the player-facing
meaning of animation index 2 or prove that every alternate-branch instance is removed or spawns
material; those outcomes also depend on the component state. In the specific GrowthComponent
threshold path, `FUN_140125520` clears the linked component's byte `+0x2d` before the alternate
dispatch, so that call queues owner removal without entering the optional clone-and-register block.

## Correct field ownership in the alternate branch

`NaturalResourceDescription+0x20` is a reference to the raw-material resource. The amount read at
`+0x40` or `+0x44` belongs to the referenced resource object resolved by `FUN_140006330`, not to the
NaturalResourceDescription itself. The branch selects which amount using byte `+0x2c` on the
NaturalResourceComponent, scales it by that component's float `+0x28` capped at `1.0`, and proceeds
only for a positive result. This corrects the ownership distinction noted in the previous step.

The component's byte `+0x2d` controls whether that optional clone-and-register path is entered.
Although it shares a nearby offset with other fields, this trace does not assign a semantic name to
the byte. The referenced material quantity fields likewise remain unnamed pending a resource and
inventory consumer trace.

## Evidence and limits

- `analysis/export/Application-steam-x64.exe/decomp.c`: `FUN_14012b2e0` serializer order;
  `FUN_14012b4e0` constructor; `FUN_14012b910` branch on description byte `+0x64`;
  `FUN_14012b5c0` optional resource clone and amount calculation; `FUN_14012a5c0` mesh animation.
- The compiled resource identity is `NaturalResourceDescription` type ID `0x3777aaa7`, registered
  in `docs/GAME-TYPE-REGISTRATIONS.tsv`. The extracted values come from the 24 matching
  `Template/*_naturalresource.crs` resources in `game-data/original/WinData/data0.pkg`.
- [GAME-LOOP-STEP-13.md](GAME-LOOP-STEP-13.md) records the cell admission and the two transition
  branches. [`RESOURCE-SEEDING.md`](RESOURCE-SEEDING.md) traces initial natural-resource setup.

## Next trace

Trace other callers that can reach `FUN_14012b5c0` with component byte `+0x2d` set. The guarded
clone path and its separate category counter are followed in
[GAME-LOOP-STEP-15.md](GAME-LOOP-STEP-15.md). Compare it with serialized entity records before
naming the amount as inventory or a harvest yield.
