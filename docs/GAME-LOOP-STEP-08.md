# Game loop step 08: status bar population and stock values

This step follows the values written by `StatusBarUI` to their source data. Evidence is static Ghidra
12.1.4 decompilation of the Steam x64 executable and its Runtime DLL. It traces the fields and
predicates used by the status bar; it does not identify every owning class or prove the localized
meaning of the three population groups.

## Population and citizen status

`FUN_14007eda0` (`0x14007eda0`) obtains an entity pointer array at `*(game + 0xF0) + 0xA0`, with
the element count at `*(game + 0xF0) + 0xA8`. Each entry is an entity object. Its component list is
at `entity + 0x10`; the update scans that list for a component matching type-info pointer
`DAT_1401CFE08`, then reads a 32-bit flag word at `component + 0xF0`. The exported type record does
not name this component. The entity shape includes a `CitizenDescription*` at `entity + 0x288`, so
the list is strongly identified as citizens, but the concrete owner and component type remain
unresolved.

The six status counts independently test bits 0 through 5 of that word. They are accumulated as
separate totals, so a citizen can contribute to more than one count and the counts need not sum to
the population.

| Flag bit | Status Bar values |
|---:|---|
| 0 | `numberStatusHome`, `imageStatusHome` |
| 1 | `numberStatusFood`, `imageStatusFood` |
| 2 | `numberStatusCold`, `imageStatusCold` |
| 3 | `numberStatusJob`, `imageStatusJob` |
| 4 | `numberStatusTool`, `imageStatusTool` |
| 5 | `numberStatusDisease`, `imageStatusDisease` |

The `textPopulation` element receives three independently formatted parameters. The three branches
are mutually exclusive and together cover the entity array:

1. `entity + 0x208` is null and `entity + 0x3C` is at least the float threshold at
   `*(entity + 0x288) + 0x44`.
2. `entity + 0x208` is null and the first branch's threshold comparison is false.
3. `entity + 0x208` is non-null, regardless of that comparison.

These are the exact predicates used for parameters 0, 1, and 2. The age/age-class interpretation of
the float comparison is plausible from the `CitizenDescription` field, but the localized labels and
the role of the pointer at `+0x208` have not been established. The three parameter totals sum to the
entity-array count.

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
name.

## Evidence pointers and remaining work

- `FUN_14007eda0` (`0x14007eda0`): iterates entities, reads citizen flags, and writes population,
  status, calendar, weather and resource values.
- `FUN_14007f8a0` (`0x14007f8a0`): binds the six status images/numbers and resource number elements.
- `FUN_14007ce40`, `FUN_14007ccb0`, and `FUN_14007cda0`: read/write/refresh the resource limit at
  table entry `+8`.
- `FUN_14004b0e0` (`0x14004b0e0`): increments the resource entry field at `+0xC` on a transfer path.
- Local Ghidra exports: `analysis/gameplay-statusbar-controller.txt` and
  `analysis/statusbar-global-owner.txt`.

Still open: identify the concrete owner and component types, map the three population parameters to
their localized labels, and trace the resource stock table's update operations back to production,
consumption and transfer. The game port still renders a compact HUD rather than these Status Bar
values.
