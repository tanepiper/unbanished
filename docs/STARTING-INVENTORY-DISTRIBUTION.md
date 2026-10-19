# Starting raw-material distribution

This trace follows the seven `RawMaterial*.rsc` candidates from each start-condition profile into
the initial object-component assignment path. Evidence is Ghidra 12.1.4 pseudocode for the Steam x64
executable and extracted `.crs` profile data. It establishes assignment counts and capacity checks,
but not the user-facing names of the seven resource-flag groups.

## Candidate list and filters

`FUN_1400a87d0` appends each path from the profile's `+0xa0` array to a unique candidate list by
calling `FUN_140097330` on the shared object at `DAT_1401cbdd8 + 0xf0`. All three profiles list the
same candidates in this order:

1. `RawMaterialWood`
2. `RawMaterialStone`
3. `RawMaterialIron`
4. `RawMaterialTool`
5. `RawMaterialPotato`
6. `RawMaterialFirewood`
7. `RawMaterialLeatherCoat`

`FUN_1400a73d0` then calls `FUN_140096b30` seven times on that object. Each call supplies one mask
and one profile integer:

| Profile field | Resource flag mask | Hard | Medium | Easy |
|---|---:|---:|---:|---:|
| `+0xfc` | `0x400` | 150 | 120 | 150 |
| `+0x100` | `0x800` | 70 | 60 | 70 |
| `+0x104` | `0x1000` | 50 | 40 | 50 |
| `+0x108` | `0x200` | 50 | 40 | 50 |
| `+0x10c` | `0x20` | 2400 | 1800 | 2400 |
| `+0x110` | `0x40` | 400 | 200 | 400 |
| `+0x114` | `0x4000` | 40 | 30 | 40 |

The masks filter candidate resource templates using the bit field at child offset `+0x5c`. The
serialized values above are the requested assignment caps passed to the distribution routine; they
are not yet mapped to labels such as building material, food, or clothing.

## Capacity-aware assignment loop

For a mask, `FUN_140096b30` builds the filtered candidate list, then walks the map object's child
components. For each eligible holding component it selects a candidate by successful-assignment
count modulo the number of candidates. `FUN_140132250(component, candidate, 1)` checks whether one
unit fits within the component's capacity. When it does, `FUN_1401324d0(component, candidate, 1)`
increments that candidate's stored quantity by one and the successful-assignment count advances.

The routine repeats the component walk while assignments are still succeeding and stops when it
reaches the profile cap or a full pass cannot add anything. This shows the seven profile integers
control total one-unit assignment attempts for their respective resource masks. They do not specify
one fixed amount for every building: actual assignments depend on which components are present,
their current contents and their capacity.

## Classes, capacities and the stock table (resolved)

- **Candidate flag field.** The candidate flag field is `RawMaterialDescription +0x5c` (serializer
  `FUN_14012e9f0`). In serialized order the description holds:
  - sprite sheet, string table, name and small-name keys;
  - four ints, including the storage size per item at `+0x48`;
  - the flags;
  - a float, an int and a bool.
- **Flags and sizes of the candidates:**

  | Raw material | Flags | Size per item |
  |---|---|---|
  | Wood | `0x400` | 11 |
  | Stone | `0x800` | 15 |
  | Iron | `0x1000` | 25 |
  | Tool | `0x200` | 10 |
  | Potato | `0x22` | 1 |
  | Firewood | `0xc0` | 4 |
  | LeatherCoat | `0x4000` | 10 |

  So each mask selects exactly one candidate. The seven caps are wood, stone, iron, tools, food,
  firewood and clothing.
- **Holding component.** The holding component is `StorageComponent`. `FUN_140132250` accepts a
  unit when `component +0x3c & flags` is non-zero and `size · n + used ≤ +0x38`. `FUN_1401323e0`
  sums `size · count` over the component's `(raw material, count)` list.
- **Storage description** (`FUN_140131e70`). Its fields are:
  - accepted flags `+0x28`;
  - per-cell flag `+0x20`;
  - activate flag `+0x21`;
  - capacity `+0x24`;
  - initial items.

  Activation (`FUN_140131b10`, when `+0x21` is set) sets capacity = `+0x24 · (per-cell ? w·h : 1)`
  and accepts = `+0x28`. The capacities are:

  | Storage | Accepts | Capacity |
  |---|---|---|
  | Barn | `0x16220` | 6000 |
  | Yard | `0x1c40` | 250 per cell |
  | Cart | `0x16260` | 2500 |
  | Houses | `0x60` | 1000 |
- **Status Bar totals.** `FUN_14008f3a0`, called every simulation iteration from the town update
  `FUN_140091780`, clears the stock table and re-sums every storage's items into it. Each raw
  material counts towards one slot, tested in this order:
  1. `0x20` → food (slot 5);
  2. bit 7 → firewood (3);
  3. bit 8 → coal (9);
  4. bit 9 → tools (4);
  5. bit 10 → wood (0);
  6. bit 11 → stone (1);
  7. bit 12 → iron (2);
  8. bit 13 → herbs (6);
  9. bit 14 → clothes (7);
  10. bit 17 → alcohol (8);
  11. bit 16 → textile (10);
  12. bits 18–27 → custom slots 11–20.

  The same pass adds storage units to two totals, split by mask `0x1c40`: building materials, and
  everything else.

### Medium result

The table below is the port's output for a Medium start:

| Storage | Units used | Contents |
|---|---:|---|
| Barn | 2500 | tools 40, food 1800, coats 30 |
| Yard (4×6) | 4020 | wood 120, stone 60, iron 40, firewood 200 |

On Hard, the cart does not accept wood, stone or iron, so those caps place nothing.

## Port

- `crates/game/src/work/stock.rs` ports `FUN_140096b30`, the capacity checks and the stock-table
  mapping.
- `apps/game/src/start.rs` fills the start structures' storages.
- The Status Bar reads the recomputed table.

### Inferred

- The town storage list (`Game +0xf0`, list `+0x00`) holds the structures with a
  `storagelocation` component, in creation order (barn, yard, cart). Houses and citizens are not
  in it. Who appends to that list has not been traced.
- The barn's storage (activate flag clear) gets the same capacity when its instant build completes.

### Remaining work

- Confirm the storage-list owner.
- Port households, which are created before this pass (`FUN_1400a62a0`).
