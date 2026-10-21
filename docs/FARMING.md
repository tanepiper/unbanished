# Farming

## Crop fields (`crates/game/src/production/farm.rs`)

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
  `RawMaterialDescription +0x4c`-th cell gets a pile. The flags are shuffled with `Rand(count)`
  swaps. Entering planting lifts the field's map stamp (`FUN_140129c20` →
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
  starting goods. Livestock picks draw too and bring their starting animals (see
  [Pastures](#pastures-cratesgamesrcpasturers)).

### Works

| Work | Try | Sequence |
|---|---|---|
| PlantArea (`0x140167cb0`) | The first cell to plant that is unclaimed (`FUN_1400f94e0`), paired with the next when that one is free too, next to a reachable cell (`0xe0f`). With nothing left to plant and no workers, unplanted cells count as done (`FUN_140120860`) | `FUN_1400c31f0`: ShowTool 3, walk (3), PerformWork 0xc 2.1 s with the tool, then for each cell CreateNaturalResource (the field as occupant) and the field's message 0, ShowTool −1, CompleteTask `(second << 16) \| first` |
| TendArea (`0x140167db0`) | A random cell (`Rand(count)`) next to a reachable cell, else 0 (`FUN_1400fa960`) | `FUN_1400c0570`: ShowTool 3, walk (3), PerformWork 0xc 10 s facing the cell, the field's message 1, ShowTool −1, CompleteTask |
| Harvest (`0x1401678c0`) | AutoPickup offers; the stock slot's limit; then like PlantArea over cells holding a crop, skipping claimed crops (`FUN_1400f21a0`) | `FUN_1400c6a00`: the removal sequence over one or two crops (no falling), with the auto-pickup flag |

Complete frees the cells. Fail frees them and marks them to do again. Refresh does the same
for cells whose worker lost the task. Cancel clears the workers' stacks.

Verification: `placement_tool::construction_tests::farmers_plant_tend_and_harvest_a_field`.
A 6 × 6 field near the start is given a known crop in spring. One farmer plants it in about 95
s, tends it, harvests it once ripe. The crop is stored.

Not ported:
- crop disease (DiseaseComponent, `FUN_1400491a0`);
- the field's status icons and tracker;
- the drag tool's own placement rules.

## Orchards (`crates/game/src/production/orchard.rs`)

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

## Pastures (`crates/game/src/production/pasture.rs`)

A pasture is placed with the fenced tool, built, and given a kind of animal in its window. It
then takes the town's waiting livestock of that kind, has young, and turns eggs and wool into
piles at its shelter. Its herdsmen cull the oldest grown animal while the herd is over the
window's limit, pick up the products, or walk the pasture; each visit lets an animal breed.
The animals themselves are AnimalComponents in `crates/game/src/nature/wildlife.rs`.

### Placement: the fenced tool

FencedTool (vtable `0x140166a10`) uses the template's CreateFencedDescription (`FUN_140048250`):
a cell-type mask (`0x14`), six parts (AnimalShelter, end, corner, straight, long straight,
gate), sizes 7–20 a side, a centred-shelter flag (off), and the Clear template.
`StartingTown::place_fenced` ports it (the drag itself is not):

- **Check** (slot `+0x70`, `FUN_1400e0960`): the size, then each cell's type, no occupant but
  a Clear marker, no zone count, height within 100.
- **Shelter** (`FUN_1400e1460`): in the corner the rotation faces (0, −90, 180, 90 degrees), its
  size from its CreatePlacedDescription (turned a quarter for rotations 1 and 3); centred at
  the middle of its corner cells, at the area's mean height (`FUN_140106c00`).
- **Place** (slot `+0x68`, `FUN_1400e02d0`): the pasture over the area (turned with the
  shelter), Clear markers in it removed (`FUN_140106da0`), then the parts in its
  MultiObjectComponent (`+0x48`): the shelter, a corner at each corner the shelter leaves
  free (`FUN_1400e1150`), and the four sides (`FUN_1400e0d90`). A side puts an end piece on
  either side of the shelter, a single straight piece where a long one does not fit, else long
  pieces. The second long piece is the gate (none on a side a centred shelter crosses). The
  pasture is registered last. A 10 × 10 pasture has 19 parts: the shelter, 3 corners, 4 gates
  and 7 long pieces, plus ends and singles.
- **MultiObject** (`0x14016a120`): it passes entity events to its parts (`FUN_14012a960`), so
  they stamp the map with the pasture (event 1, state 2 of the ClearArea controller). Its
  removal destroys them (`FUN_14012ac90`). The parts' InteractComponents are children of the
  pasture's (`FUN_140128540` / `FUN_1401282f0` count and index through them), so the
  shelter's points are the pasture's build and create points.

### PastureComponent (states `FUN_14012d660`)

Fields: `+0x28` kind (animal template, none at setup), `+0x30` animals, `+0x40` the limit (the
window's slider), `+0x44` products gathered. The PastureDescription's `+0x20` (stock slot,
factory default 5) limits the products.

- **State 2 enter** (`FUN_14012c590`): take waiting livestock, join the town's pastures
  (`+0x90`), disable the workplace.
- **State 2 update** (`FUN_14012bcb0`):
  - Take waiting livestock of the kind, in the town list's order, until full (`FUN_14012c610`).
  - Births (`FUN_14012c840`): each animal with young due has them while the pasture holds
    fewer than its room plus three.
  - The cells remember the kind (`FUN_1400a4c40`).
  - Products: while more than a pile's worth (RawMaterialDescription `+0x40`) has gathered, a
    pile's worth is taken. Below the stock limit it becomes a pile on the first shelter point
    without one (anything else there is cleared), turned `RandTol(2) × 90`, registered at once
    and handed to the AutoPickup. Otherwise it is lost.
- **State 3 enter** (`FUN_14012c310`) and removal (`FUN_14012ccb0`): every animal goes back to
  wait, last first, and the pasture leaves the town's list.
- **Event 4** (`FUN_14012ca10`, sent when building ends, `FUN_14010cad0`): the shelter points'
  cells become plain ground.
- **Room** (`FUN_14012da40`): area over AnimalDescription `+0x24` (cells per animal: 20 for
  cattle).
- **Window** (PastureUI, `0x140165a20`):
  - The kind list (`FUN_1400ba110`, `Command::SetCrop`) is only enabled while the pasture is
    empty. Choosing the first kind switches the work on. The limit becomes the room.
  - The slider (`FUN_1400ba2f0`, `Command::SetLimit`).
  - Buttons (`FUN_14012cf30`, `Command::FieldAction`), each available per `FUN_14012d2a0`:
    - 0 sends every animal to other pastures of the kind with room, the roomiest first;
    - 1 sends half of them, rounded down;
    - 2 switches the work on or off, making or removing it.
- **Message 0** (`FUN_14012cbf0`, a herdsman's visit): a random animal is marked to breed.

### Livestock

- **LivestockDescription** (`FUN_140129430`): product (eggs, wool; none for cattle), every
  `+0x2c` × 300 s, stock slot.
- **LivestockComponent**: `+0x28` pasture, `+0x30` timer.
  - Without a pasture it waits in the town's list (`+0x80`, activation `FUN_140129140`).
  - Its update (`FUN_140129020`) adds one product to its pasture per interval.
  - Joining a pasture (`FUN_140129190`) ends the wild flag and keeps it to the pasture's area
    (`FUN_140129570`). Leaving one makes it wild again where it stands (`FUN_14010b3a0`).
- **Breeding** (AnimalComponent's update `FUN_14010b8f0`): a male marked to breed and past its
  grown age (`+0x30`) waits `(RandFloat × breed + breed)` years (`+0x34`, 0.6 for cattle).
  Then young are due (`0x200000`).
- **Young** (`FUN_14012db40`):
  - Placed next to the parent, or anywhere in the pasture.
  - Male with a chance equal to the herd's share of females (always the first).
  - Turned `RandFloatTol × 180`, age 0 (any age under a year without a parent).
- **Starting livestock** (`FUN_1400a5fe0`): for each livestock pick (Easy: one), eight
  animals.
  - Placed around the start point plus the profile's offset (`+0xec`, `+0xf0`), spread
    `+0xf4`.
  - Males and females in turn, from a random first; any age under a year.
  - Wild, they wait for a pasture.

### The herdsmen's work (vtable `0x140167bc0`)

Slot `+0x20` (`FUN_1400f7840`), after the AutoPickup offers:

1. Over the most workers, or with the stock at its limit, "not now".
2. Over the limit: the oldest animal past its grown age that no one has taken
   (`FUN_14012d820`) is culled with `FUN_1400c1370`. The herdsman walks next to it, works 4 s,
   removes it, and leaves its raw materials (beef, leather) where it stood.
3. Else the products' pickups at the shelter are offered (a second nested round of offers;
   `Jobs::try_one` now loops while a resume answers `NESTED`).
4. Else the herdsman walks to a random cell of the pasture and tends (`FUN_1400c0570`), sending
   message 0.

The other slots:

- Complete (`FUN_1400f7820`) leaves the workers' set.
- Fail (`FUN_1400f7790`) also frees a culled animal.
- Cancel (`FUN_1400f8060`) and refresh (`FUN_1400f7560`) work as the gatherer's do.

Not ported:

- Status icons, the tracker and the production counter.
- The debug fill `FUN_140060bf0`.

Verification:

- `placement_tool::construction_tests::herdsmen_keep_a_pasture`: Easy start, 10 × 10 pasture.
  It is built and takes 5 of the 8 starting cattle. A visit marks a breeder, young are born,
  and with the limit at 2 the herdsman culls down to it, leaving meat.
- `a_pasture_gathers_eggs`: hens in a pasture lay egg piles at the shelter, which are hauled
  to storage.

## Blight (`crates/game/src/disasters/blight.rs`)

Fields, orchards and pastures carry a DiseaseComponent (vtable `0x14015f3b0`). Fields and
pastures share its DiseaseDescription (`FUN_1400497a0`) layout:

| | Crop field | Pasture |
|---|---|---|
| Months it can start (`+0x20..+0x24`) | 3–7 | 0–12 |
| Odds (`+0x28`, `+0x2c` when the same thing grew last) | 200, 150 | 200, 150 |
| Check every (`+0x30` × 300 s) | 0.25 | 0.25 |
| A death every (`+0x34` × 300 s) | 0.03 | 0.2 |
| A spread every (`+0x38` × 300 s), within (`+0x40` cells) | 0.125, 20 | 0.125, 40 |
| Lasts (`+0x3c` × 300 s) | 8 | 12 |

- **Starting** (`FUN_140048f70`): while healthy and in its months, a check every interval.
  The odds are one in `12 / check × odds`; `odds_same` applies when a cell's last growth
  (`+0x10`) is what grew when the blight last ended (`FUN_1400a5030`). It only takes a built
  site with something to blight (`FUN_140048d00`). That posts event 12 at severity 3 with the
  site's toolbar name, which the game logs from `Construction::site_events`.
- **While sick** (`FUN_1400494f0`):
  - The time left runs down.
  - It ends (`FUN_140048c80`, remembering what grows now) when the time runs out or nothing
    is left (`FUN_140048b10`: no animal, or no natural resource on the cells).
  - Each death interval kills one random target, up to 50 draws:
    - a pasture: an animal no one has taken (`FUN_1400486d0`);
    - otherwise a natural resource no one is removing (`FUN_1400484a0`). It withers: growth
      0, no pile, flagged for removal, clip 2 when it has one.
  - Each spread interval (`FUN_1400494f0`) picks a random cell within the radius of the
    centre. The field or pasture there (occupant, else object) catches it when it grows the
    same thing (`FUN_1400488e0`: the pasture's kind, the first natural resource, else the
    crop or tree).

Not ported:

- the particles;
- the debug flag that stops it (`Game +0xd0` bit 5, set from the console);
- the town's list of DiseaseComponents (`+0xf0`).

The updates run every iteration with the sites (see [JOBS.md](JOBS.md)).

Verification: `placement_tool::construction_tests::blight_withers_a_field_and_spreads`. Two 10
× 10 wheat fields; the first is made sick (with 200 spread intervals banked). Its crops wither,
the second field catches it. The event is posted.

