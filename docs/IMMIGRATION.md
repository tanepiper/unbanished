# Immigration and nomads

Port: `crates/game/src/immigration.rs`; the citizens are made in `Agents::admit_band`
(`crates/game/src/agents.rs`). Test: `nomads_are_taken_in` (`apps/game/src/placement_tool.rs`).

## ImmigrationDescription

`Template\TownHall.rsc:immigration` (serializer `FUN_14004f150`, constructor `FUN_14004f300`
with its defaults):

| Offset | Meaning | TownHall | Default |
|---|---|---|---|
| `+0x20` | the nomad template | `Template\Nomad.rsc` | |
| `+0x28` | the population needed | 8 | 80 |
| `+0x30` | templates the town must have | Market, TradingPost | |
| `+0x40`, `+0x44` | the band's share of the population | 0.05, 0.15 | same |
| `+0x48`, `+0x4c` | months to the next band: `+0x48 + RandFloat × +0x4c` | 24, 48 | same |
| `+0x50`, `+0x54` | the months a band may come in | 1, 3 | same |
| `+0x58` | from this month a waiting band leaves (not serialized) | | 7 |
| `+0x5c` | the map-edge margin (not serialized) | | 10 |
| `+0x60`, `+0x64` | the visitors' disease risk and its months | 1.0, 6 | same |

"Must have" is the town manager's zones by template (`+0x160`, filled by ZoneComponent
activation `FUN_1401356c0` → `FUN_14008e2f0`): a placed Market and Trading Post, built or not.

## ImmigrationComponent

Vtable `0x14015fc50` (constructor `FUN_14004efa0`): `+0x48` until a band may come, `+0x50` the
nomads arrived, `+0x58` the band. The town hall's last controller (after Destroy). States
(`FUN_14004f020`):

| State | Enter / update | Transitions | Messages |
|---|---|---|---|
| 2 | `FUN_14004dbf0` / `FUN_14004db80`: when the town qualifies (`FUN_14004ed50`: population `+0xa8` ≥ `+0x28` and every required template placed) → 0xf | 3, 0xf | |
| 0xf | `FUN_14004db00`: `+0x48 = (RandFloat × +0x4c + +0x48) × 300 s`. `FUN_14004da30`: the time runs down; when out and the month is within `+0x50..+0x54`, → 0x10 if the town still qualifies, else 2 | 3, 0x10, 2 | |
| 0x10 | `FUN_14004d9f0`: a new band (`FUN_14004e540`). `FUN_14004d9d0`: again while none could be placed | 3 (`FUN_14004d660`: the band leaves), 0x11 | 6 (`FUN_14004d950`): one more arrived; all of them → 0x11 |
| 0x11 | `FUN_14004d810`: event 14 at severity 2 (colour `0xff8080ff`). `FUN_14004d760`: from month `+0x58`, the band leaves and → 2 | 3 (`FUN_14004d660`), 2 | 7 (`FUN_14004d6f0`): taken in (`FUN_14004ddb0`), → 2; 8 (`FUN_14004d680`): the band leaves, → 2 |

The months are `seconds / 3600 × 12` of the calendar, past 11 counting as 0. Messages 7 and 8
come from ImmigrationUI's buttons (events `0x9858` → `FUN_14006b9e0`, `0x9859` →
`FUN_14006b930`); the UI shows them while the state is 0x11 (`FUN_14006ba90`).

**A band** (`FUN_14004e540`): up to ten tries at an edge: `Rand(4)` picks the side (0 left at
`x = +0x5c`, 1 right at `n − +0x5c − 1`, each with `y = Rand(n)`; 2 top, 3 bottom likewise with
`x = Rand(n)` first) and `FUN_14004ebd0` must find a cell near it: up to 50 tries of `RandTol(+0x5c)`
offsets (x, then y), clamped, walkable (`0xe07`), and either a water type on either end (types
5..8) or the same non-zero region as the town hall's use point 0. The band has `Rand(2 × empty
family slots of the registered residences) + (int)(population × (RandFloat × (+0x44 − +0x40) +
+0x40))` nomads, at least two; each starts at a new edge cell (or the first one), and is bound
for one of the town hall's create points (`Rand(count)`, its own then its parts').

**The nomad** (`FUN_140050990`): `+0x28` the town hall, `+0x30` where it starts, `+0x3c` where it
waits, `+0x50 = Rand(2)` its variant (0 men, 1 women; material `+0x28` or `+0x30` of the
NomadDescription), placed at the start turned `RandFloatTol × 180`. Its states (`FUN_1400507d0`):

| State | Enter / update | Messages |
|---|---|---|
| 0 | `FUN_140050450`: walk to `+0x3c` (`FUN_1400504c0`: stack cleared, MoveTo walk 1, flags `0xe1f`, a random speed). `FUN_140050320`: once idle, without a town hall it is gone, else message 6 to it and → 1 | 1 (`FUN_14004fe00`): → 2, the town hall forgotten |
| 1 | — (`FUN_140050310`). `FUN_14004ff60`: once idle, every `RandFloatTol × 15 + 30` s a ground cell (types 0..2) of the 16 × 16 around `+0x3c`, `RandFloatTol × 0.33` off its centre (y then x) | 0 (`FUN_14004fd80`): gone; 1 as above |
| 2 | `FUN_14004ff40`: walk back to `+0x30`. `FUN_14004fe90`: once idle, gone | |

**Taken in** (`FUN_14004ddb0`): the town's visitor risk `+0x1a8` becomes at least `+0x60` and its
time `+0x1a0` at least `+0x64 × 300 s` (the town step `FUN_140091780` counts it down and clears
both when it runs out; citizens' disease odds read it); `+0x1b0` counts the nomads taken in.
Then in fours, each made where its nomad stands (`FUN_140050b00`: the NomadDescription's citizen
template, `FUN_14011c550` as a laborer): a man, a woman married to him, a girl and a boy in her
care. Adults are `RandFloat × (+0x54 − +0x50) + +0x50` (CitizenDescription) years old, children
`RandFloat × +0x50`; a `Rand(1)` is drawn between the age and the citizen's setup. Each nomad
gets message 0.

## Port notes

- Nomads are kept with the wildlife (`Wildlife::nomads`) and moved with the animals; their
  StateMachine updates run after the sites'.
- A band taken in becomes citizens after the frame's other commands.
- Not ported: the nomads' material instances, ImmigrationUI and TownHallUI.
