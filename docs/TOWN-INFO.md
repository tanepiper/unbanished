# Town statistics and production tracking

Port: `crates/game/src/work/tracker.rs` (TrackerComponent, the stock manager's counters and its
history), sampled from `apps/game/src/gameplay.rs`. Tests: `tracker::tests`,
`cutter_yard_makes_firewood`.

## What is made and used

Each of the stock manager's 21 slots (`Game +0xf0 → +0x170`, 0x20 bytes) has `+0x00` the stock
count, `+0xc` what was used and `+0x10` what was made since the last sample. A raw material
counts towards the slot of its first flag (`FUN_14008e460`, `produce::production_group`).

| Made (`+0x10`) | Where |
|---|---|
| CreateRawMaterial (`FUN_1400cc010` → `FUN_14008e3c0`; `+0x51` always set) | produce, fishing, orchards, hunting: the workplace's tracker too |
| a natural resource's pile (`FUN_14012b5c0` → `FUN_14008e3c0`) | felling, mining, harvesting: the citizen's workplace's tracker when the RemoveNaturalResource action's `+0x30` is set |
| a pasture's product (`FUN_14012bcb0`) | the pasture's tracker |

| Used (`+0xc`) | Where |
|---|---|
| ConsumeRawMaterial (`FUN_1400cb420`): each item's amount | production inputs, herbs, tools, coats |
| a citizen eating (`FUN_14011a7b0`): one a unit | |
| building materials (`FUN_14010d4c0`, BuildComponent::Finish): what the site held | |
| a happiness visit (`FUN_14004b0e0`): one | tavern, market |
| automatic trading (`FUN_14005a430`): one an item paid | |

**TrackerComponent** (factory `FUN_14000d7a0`, no behaviour of its own; workplaces with a
`tracker` description): `FUN_140059930(tracker, material, amount)`: in a new year (`Game +0x128`)
the `+0x28` list (material, amount) becomes `+0x38` and starts again; the material's amount
grows, or it joins the list.

## The history

StockManager::Update (`FUN_140091780`) counts `+0x198` down; below 1 it samples
(`FUN_14008fc00`) and waits 75 s (the first sample is at the first update). A sample appends to
71 series (`+0x150`, an array each):

| Series | Value |
|---|---|
| 0 | citizens |
| 1, 2, 3 | adults (of age, not at school), students, children |
| 4 | average health over CitizenDescription `+0x70`, percent |
| 5 | educated adults (`+0x210 ≥ +0x118`) over adults, percent (no adults: `i32::MIN`, the float conversion of a NaN) |
| 6 | average happiness over `+0xa4`, percent |
| 7 | citizens with a coat (`+0x1e4`), percent |
| 8 + 3k, 9 + 3k, 10 + 3k | slot k's count, used and made |

With no citizens, series 4 to 7 get 0. Then every slot's used and made restart.

The window that draws these is TownHallUI ([BUILDING-WINDOWS.md](BUILDING-WINDOWS.md)). The
300 s clock beside the sampler (`+0x190`, `+0x1b8`) runs the commute optimisation
([COMMUTE.md](COMMUTE.md)).
