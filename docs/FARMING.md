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
