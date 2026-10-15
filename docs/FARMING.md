# Farming

## Crop fields (`crates/game/src/farm.rs`)

A crop field is a dragged area (`StartingTown::place_area`, the end of the CreateDrag tool).
Its components run in this order: ClearArea, WorkPlace (farmer), CropField. It is a site like
a building, but has no BuildComponent.

### CropFieldComponent (states `FUN_140120620`)

| State | Enter | Scheduled update | Exit |
|---|---|---|---|
| 2 fallow | workplace on/off with the field (`FUN_14011fdf0`) | switched on, at least 33 °F and months 0–2: plant (`FUN_14011fd30` → `FUN_14011fc80`) | |
| 9 planting | the cells, then PlantArea (`FUN_14011f7e0`) | all lost → 0x15; all planted or lost → 10 (`FUN_14011f6b0`) | |
| 10 growing | TendArea (`FUN_14011f5d0`) | all lost → 0x15. With piles and switched on, harvest when ripe (mean growth > 0.99) or warm in months 6–11 (`FUN_14011f4a0`) | the work goes (`FUN_14011f440`) |
| 11 harvest | the harvest work (`FUN_14011f330`) | all done → 0x15 (`FUN_14011f250`) | |

State 0x15 belongs to ClearAreaComponent and is the same as its state 0: clear the
footprint, then go through 1 to 2.

- **Cells** (`FUN_14011f7e0`). Each cell holds `+0` the map cell, `+4` its state (0 to plant,
  1 growing, 2 done), `+8` the crop, and `+0x18` whether harvesting it leaves a pile. Every
  `RawMaterialDescription +0x4c`-th cell gets a pile, and the flags are shuffled with
  `Rand(count)` swaps. Entering planting lifts the field's map stamp (`FUN_140129c20` →
  `FUN_1400a33b0`), so crops can stand on its cells.
- **Messages** (`FUN_140120020`):
  - 0: a cell was planted. It becomes growing, and its crop's NaturalResource `+0x2d`
    takes the pile flag.
  - 1: a tending visit. The crop's GrowthComponent update runs for
    `growing time × 300 s × CropFieldDescription +0x24` (0.005).
- **The field's buttons** (`FUN_140120190`, `Command::FieldAction`):
  - 0 plant now (message 3, handled in state 2);
  - 1 harvest now (message 4, handled in state 10);
  - 2 switch farming on or off.

  Choosing the first crop in CropUI (`FUN_1400b7df0`, `Command::SetCrop`) also switches
  farming on. A new field has no crop (`FUN_140120330`).
- **Known crops.** At map setup, `crop_picks` random crops and `tree_picks` random orchard
  trees from the profile join the town's known list (`FUN_140096ef0`). This happens after the
  starting goods. Livestock picks draw too, but the starting livestock (`FUN_1400a5fe0`) is not
  ported.

### Works

| Work | Try | Sequence |
|---|---|---|
| PlantArea (`0x140167cb0`) | The first cell to plant that is unclaimed (`FUN_1400f94e0`), paired with the next when that one is free too, next to a reachable cell (`0xe0f`). With nothing left to plant and no workers, unplanted cells count as done (`FUN_140120860`) | `FUN_1400c31f0`: ShowTool 3, walk (3), PerformWork 0xc 2.1 s with the tool, then for each cell CreateNaturalResource (the field as occupant) and the field's message 0, ShowTool −1, CompleteTask `(second << 16) \| first` |
| TendArea (`0x140167db0`) | A random cell (`Rand(count)`) next to a reachable cell, else 0 (`FUN_1400fa960`) | `FUN_1400c0570`: ShowTool 3, walk (3), PerformWork 0xc 10 s facing the cell, the field's message 1, ShowTool −1, CompleteTask |
| Harvest (`0x1401678c0`) | AutoPickup offers; the stock slot's limit; then like PlantArea over cells holding a crop, skipping claimed crops (`FUN_1400f21a0`) | `FUN_1400c6a00`: the removal sequence over one or two crops (no falling), with the auto-pickup flag |

Complete frees the cells. Fail frees them and marks them to do again. Refresh does the same
for cells whose worker lost the task. Cancel clears the workers' stacks.

Verification: `placement_tool::construction_tests::farmers_plant_tend_and_harvest_a_field`.
A 6 × 6 field near the start is given a known crop in spring. One farmer plants it in about
95 s, tends it, harvests it once ripe, and the crop is stored.

Not ported:
- crop disease (DiseaseComponent, `FUN_1400491a0`);
- the field's status icons and tracker;
- the drag tool's own placement rules.

## Orchards (`crates/game/src/orchard.rs`)

An orchard is a dragged area with the components ClearArea, WorkPlace (farmer) and Orchard.

### OrchardComponent (states `FUN_140052a40`)

| State | Enter | Scheduled update | Exit | Messages |
|---|---|---|---|---|
| 2 off | workplace off (`FUN_140051e90`) | switched on → 9 (`FUN_140051e20`) | | 3: → 9 |
| 9 planting | tree slots, footprint lifted, PlantOrchard (`FUN_1400519d0`) | all standing → 10 (`FUN_1400518f0`) | | 5: clear (0x14) |
| 10 tending | lost trees → 12, else TendArea (`FUN_140051790`) | harvest when due; lost trees → 12; tending only in months 1–8 (`FUN_140051560`) | the work goes | 4: harvest now, 5 |
| 12 replanting | PlantOrchard (`FUN_140051440`) | harvest when due; none lost → 10 (`FUN_140051370`) | the work goes | 4, 5 |
| 11 picking | the picking work; cells remember the tree (`FUN_140051160`, `FUN_1400a4c40`) | every standing tree picked → 10 (`FUN_140050fb0`) | | 5 |
| 0x14 clearing | a ClearArea work (`0x401`) (`FUN_140050e40`) | cleared → 9 (`FUN_140050d90`) | the work goes | |

- **Tree slots.** A slot sits every `OrchardDescription +0x24 / +0x28` cells (2 × 3). Each
  holds its cell, its state (0 to plant, 1 standing, 2 lost), the tree, and a picked flag.
- **Harvest due** (`FUN_140051ee0`). The orchard must be switched on and not yet harvested
  this year. It is then due either when warm in months 7–11 with any fruit, or when the mean
  ripeness is above 0.99 (`FUN_140051fa0`).
- **Messages** (`FUN_1400522f0`):
  - 0: a slot was planted. It stands if the tree has fruit; otherwise it is lost.
  - 1: a tree was picked. Its fruit starts again.
  - 2: a tending visit ripens the fruit by `ripening time × 300 s × OrchardDescription +0x2c`.
- **FruitComponent** (`FUN_14004a740`). A tree's fruit ripens once the tree is grown, at
  `1 / (months × 300)` per second scaled by `1 − ((t − best) / range)²`. Outside the range it
  is 0, and −10 at or below 32 °F. A picked tree waits until months 7–11 are over. It runs
  after the tree's growth in the bucket pass.
- **Works:**
  - PlantOrchard (`FUN_1400f9fc0`) plants the first empty, unclaimed slot through
    `FUN_1400c3bb0`, with the orchard as occupant and its message 0.
  - TendArea tends a random tree, sending message 2.
  - Picking (`FUN_1400f2d90`) visits every standing tree (`FUN_1400c0c80`): walk, work 1 s,
    leave fruit scaled by its ripeness on even-indexed trees only, then message 1.
- **Controls.** The orchard's buttons (`FUN_140052500`) and tree list reuse
  `Command::FieldAction` and `Command::SetCrop`.

Verification: `placement_tool::construction_tests::arborists_plant_tend_and_pick_an_orchard`.
A 7 × 7 orchard gets 12 trees, planted in about 74 s. Fast-forwarded to grown trees with ripe
fruit in autumn, the fruit is picked and stored.
