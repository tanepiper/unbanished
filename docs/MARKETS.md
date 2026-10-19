# Markets

Port: `crates/game/src/commerce/market.rs`. Evidence: Ghidra pseudocode and disassembly of the Steam x64
executable (`FUN_1400f5450` and its helpers), the shipped `Template/Market.rsc`.

## Components

- **MarketLocationComponent** (states `FUN_14004fa70`). State 2 enter (`FUN_14004f880`)
  appends the market to the town's markets (`+0x10`); state 3 enter (`FUN_14004f840`) removes
  it. Both pass the chain on. The town's storage search (`FUN_140092a60`) looks at the
  ordinary storages, then the markets within their radius (`RadiusDescription`, 45 cells).
  The port queues the change with the storage locations' (`Construction::storage_changes`).
- **MarketComponent** is a ResourceProduceComponent (vtable `0x14015fd60`). Its slot `+0x90`
  (`FUN_14004f3a0`) makes the MarketWork that state 0xd registers.
- **MarketDescription** (`FUN_14004f510`): the fill threshold 0.99, then 11 categories (raw
  material flags, share of the market's room): the five foods 0.168 each, then 0.02–0.06 for
  clothes, tools, firewood, herbs, ale and the rest.

## MarketWork (vtable `0x140167b40`)

`+0x48` maps each vendor to its plan: the materials and counts on their way, or nothing for
a vendor carrying goods away. Complete and fail (`FUN_1400f5430`, `FUN_1400f5420`) drop the
vendor's plan. Cancel (`FUN_1400f7170`) clears every vendor's stack. Refresh
(`FUN_1400f51f0`) drops the plans of vendors no longer on it.

**The try** (`FUN_1400f5450`):

1. As many vendors as places: "not now". A vendor carrying anything drops it off first
   (`FUN_14011b330`).
2. A market fuller than the threshold: "not now".
3. **Tallies.** Per category (flags matching the material's), in units (`size × count`):
   - what the market holds;
   - what is on its way (the plans);
   - what the town's ordinary storages hold, except workshops' (ConsumeProduce).
4. **Category values.** The fill is `(held + on its way) / room`, and the ratio is that over
   the share. The room left is `room × share − held − on its way`.
5. **Too much.** For each category holding more than twice its share, in order: the
   market's goods of it above 1.25 times the share. As much as the vendor carries goes to a
   storage that takes it (`FUN_140092a60`, depositing, not the market), from a random use
   point of the market. Return.
6. **Sort.** The categories are sorted by ratio, least stocked first (`FUN_14002bfe0`, the
   CRT quicksort).
7. **Pick a storage.** Every ordinary storage except workshops is considered. Its entrance
   (`FUN_14008d480`) must be reachable. Its score is the sum over the sorted categories of
   `(1 − ratio) × min(units there, carrying room) / (index + 1)`, divided by the path rank
   (`n +` distance without a path). The best score wins; none above 0 means "not now".
8. **Candidates.** Each of the storage's goods whose first matching sorted category is under
   twice the share is a candidate. The share is read at the sorted index, as the original
   does. Candidates are sorted by what the market holds of the material (as it tallied:
   `size × count²` per stack) plus what is on its way, least first.
9. **Plan.** In that order, each candidate takes as much as fits the vendor's room and the
   category's room left. The vendor walks (with a tool: walk clip 5, work steps spending one)
   from the storage's drop point nearest it (`FUN_140132aa0`) to a random use point of the
   market. All the goods come in one pickup (`FUN_1400cf700`, reserving each) and one
   drop-off.

Test: `placement_tool::construction_tests::vendors_stock_a_market`. An Easy start's market is
built, joins the town's markets with radius 45, and its vendors bring goods.

Not ported: the market's ambient sound and particles, its window (the work toggle buttons
are the WorkPlace's).
