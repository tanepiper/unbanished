# Game loop step 08: status bar population and stock values

This step follows the values written by `StatusBarUI` to their source data. Evidence is static Ghidra
12.1.4 decompilation of the Steam x64 executable and its Runtime DLL plus decoded UI resources. It
traces the fields and predicates used by the status bar; the producers of citizen flags and stock
updates remain unidentified.

## Population and citizen status

`FUN_14007eda0` (`0x14007eda0`) obtains an entity pointer array at `*(game + 0xF0) + 0xA0`, with
the element count at `*(game + 0xF0) + 0xA8`. Each entry is an entity object. Its component list is
at `entity + 0x10`; the update scans that list for a component matching type-info pointer
`DAT_1401CFE08`, then reads a 32-bit flag word at `component + 0xF0`. The type record initialized by
`FUN_140155910` (`0x140155910`) identifies this component as `StatusIconComponent`, derived from
`Component`; its constructor `FUN_140131590` zeroes the flag word. The entity shape includes a
`CitizenDescription*` at `entity + 0x288`, so the iterated entries are citizens, while the concrete
owner of the entity array remains unresolved.

The six status counts independently test bits 0 through 5 of that word. They are accumulated as
separate totals, so a citizen can contribute to more than one count and the counts need not sum to
the population.

| Flag bit | Status Bar values |
|---:|---|
| 0 | Home: `numberStatusHome`, `imageStatusHome` |
| 1 | Food: `numberStatusFood`, `imageStatusFood` |
| 2 | Cold: `numberStatusCold`, `imageStatusCold` |
| 3 | Job: `numberStatusJob`, `imageStatusJob` |
| 4 | Tool: `numberStatusTool`, `imageStatusTool` |
| 5 | Disease: `numberStatusDisease`, `imageStatusDisease` |

The paired `Dialog/StringTable.rsc:gameDialogs` tooltip keys (`StatusHomeTip` through
`StatusDiseaseTip`) describe these as citizens without homes, hungry, freezing, without jobs,
without tools, and with diseases, respectively. The matching per-citizen tips confirm the same
ordering. The consuming path and resource labels identify the bits' meanings, but the code that sets
them has not been found.

The `textPopulation` element receives three independently formatted parameters. The three branches
are mutually exclusive and together cover the entity array:

1. `entity + 0x208` is null and `entity + 0x3C` is at least the float threshold at
   `*(entity + 0x288) + 0x44`.
2. `entity + 0x208` is null and the first branch's threshold comparison is false.
3. `entity + 0x208` is non-null, regardless of that comparison.

The element `Dialog/StatusBar_textPopulation.crs` selects `Dialog/StringTable.rsc:gameDialogs` key
`Population`, whose format is `@0 / @1 / @2`; `PopulationTip` labels them as adults, students, and
children. The decompiled writes therefore map parameter 0 to Adults, parameter 1 to Students, and
parameter 2 to Children. The adult predicate is the threshold comparison with a null `+0x208`
pointer; the student predicate is non-null `+0x208`; the child predicate is a failed threshold
comparison with null `+0x208`. The three parameter totals sum to the entity-array count. This is
English resource evidence; locale variants have not been inspected.

## Resource stock values

Stock counts are read from a separate table reached through `game + 0xF0`, then owner `+0x170`.
Each resource entry occupies `0x20` bytes, and the Status Bar displays the integer at entry offset
`+0`.

| Index | Entry offset | Status Bar element |
|---:|---:|---|
| 0 | `0x000` | `numberWood` |
| 1 | `0x020` | `numberStone` |
| 2 | `0x040` | `numberIron` |
| 3 | `0x060` | `numberFireWood` |
| 4 | `0x080` | `numberTools` |
| 5 | `0x0A0` | `numberFood` |
| 6 | `0x0C0` | `numberHerbs` |
| 7 | `0x0E0` | `numberClothes` |
| 8 | `0x100` | `numberAlcohol` |
| 9 | `0x120` | `numberCoal` |
| 10 | `0x140` | `numberTextile` |
| 11–20 | `0x160`–`0x280` | `numberCustom0`–`numberCustom9` |

Other routines use `+8` in each entry for the editable resource limit. A transfer/pickup path at
`FUN_14004b0e0` increments `+0xC`; the meaning of that field is not established here. The concrete
type of the object at `game + 0xF0` and the resource-table class are also not identified, so
“inventory manager” is a convenient description of the observed data path, not a confirmed type
name. A separate natural-resource path increments entry `+0x10`; its guard, category selection and
entity-clone path are traced in [GAME-LOOP-STEP-15.md](GAME-LOOP-STEP-15.md), but that counter's
meaning remains unresolved.

## Evidence pointers and remaining work

- `FUN_14007eda0` (`0x14007eda0`): iterates entities, reads citizen flags, and writes population,
  status, calendar, weather and resource values.
- `FUN_14007f8a0` (`0x14007f8a0`): binds the six status images/numbers and resource number elements.
- `FUN_14007ce40`, `FUN_14007ccb0`, and `FUN_14007cda0`: read/write/refresh the resource limit at
  table entry `+8`.
- `FUN_14004b0e0` (`0x14004b0e0`): increments the resource entry field at `+0xC` on a transfer path.
- Local Ghidra exports: `analysis/gameplay-statusbar-controller.txt` and
  `analysis/statusbar-global-owner.txt`.

The status flag producers are traced in [step 09](GAME-LOOP-STEP-09.md). Still open: identify the
concrete owner of the entity array and stock table, trace the Status Bar value at row `+0` through
production, consumption and transfer, explain the separate `+0x10` counter, and resolve the tool-bit
transition in the separate event path. The port still renders a compact HUD rather than these
Status Bar values.
