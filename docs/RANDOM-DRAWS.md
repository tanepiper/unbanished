# Random draws: every site and its state

The simulation draws all its random numbers from one generator, the Mersenne Twister at
`Game +0xd8` (`DAT_1401cbdd8 + 0xd8`). A port that draws a different number of times, or in a
different order, drifts away from the original, so every function that draws is a place to check.
This page lists them and what is known about each. Evidence is *observed* (read in the code or the
machine code) or *measured* (the original run under the emulator, with a test that compares the
port).

`python3 tools/triage/draw_sites.py` lists the functions from the Ghidra export: each function
that calls a `System::Random` method on the game's generator, with the number of calls, and
whether the port cites it and whether an emulator script or test names it. `--missing` leaves out
the measured ones. On 2026-10-23 it lists 70 functions: 16 measured, 51 ported, 3 not ported.

## Not ported

| Function | What it is |
|---|---|
| `FUN_140060770`, `FUN_140060810`, `FUN_140060890` | Buttons of the debug window: start a blight, a fire or a disease. Only `FUN_140061520` (the debug window) calls them. They are not part of normal play |

## Measured

| Function | Rule | Check |
|---|---|---|
| `FUN_1400ae760` | A chance of once in a span of years (injury, tornado, fire that starts by itself) | `tools/emu/once_in_years.py` |
| `FUN_140117ca0` | A healthy citizen's disease check; no draw when the odds are 0 or less | `tools/emu/disease_check.py` |
| `FUN_140117880` | The end of an illness: recover or die | `tools/emu/recovery.py` |
| `FUN_1400ce640` | A walk's speed factor; a NaN speed is kept with no draw | `tools/emu/move_speed.py` |
| `FUN_14005be50`, `FUN_14005bdb0` | A trading post's row and the time to the next merchant | `tools/emu/trade_timer.py` |
| `FUN_140098ac0` | The weather pick and the yearly climate reseed | `tools/emu/weather_pick.py` |
| `FUN_140094410` | A storage's item choice | `tools/emu/storage.py` |
| `FUN_14004f710`, `FUN_14008d480` | Storage geometry and an entrance cell | `tools/emu/storage_geometry.py` |
| `FUN_140126280`, `FUN_140125070` | Growth | `tools/emu/growth.py` |
| `FUN_140129f90` | A model's variant | `tools/emu/spawn.py` |
| `FUN_1400a95b0` | The music player | `tools/emu/game_loop.py` |

`draw_sites.py` also counts `FUN_1400560e0`, `FUN_1400592e0`, `FUN_14010fa30` and
`FUN_140117af0` as measured, because an emulator script names them.

## Read against the port (observed)

Each of these was compared with the original's code, draw by draw: the number of draws, their
order, the conditions that skip them, and the arithmetic on the result.

| Function | What it is | Result |
|---|---|---|
| `FUN_140117940` | A sick citizen passes the disease on | Differed: the original offers it to one citizen only (the loop exits after the call at `0x140117aae`). Fixed in `9385b0f` |
| `FUN_1401108b0`, `FUN_140111160` | Fetching firewood and food for home | Differed: the original makes both walks (`FUN_1400c4c60`, each drawing a speed) before it chooses the goods. Fixed in `8d19bc0` |
| `FUN_14008d310` | An animation's sound event | Differed: the port drew only for trees; citizens' tools and livestock draw too (`MeshInstance::Update`, Runtime `0x180103090`). Fixed in `08cc127`; the order within the iteration is part of issue #22 |
| `FUN_1400581c0` | A tombstone's lifetime | The formula matches; the original draws it inside the death (`FUN_14011c2a0` → `FUN_1400473f0` → the entity constructor's setup). The port draws it later. Issue #23 |
| `FUN_1400475c0`, `FUN_14008ec10` | A free grave and the cemetery search | Match, including `Rand(0)`, which draws nothing |
| `FUN_140048f70`, `FUN_1400484a0`, `FUN_1400491a0` | Blight: the chance, a crop that withers, the spread | Match |
| `FUN_14004db00` | An immigration wait | Match |
| `FUN_1401228d0` | A building being destroyed collapses | Match |
| `FUN_14011e940` | A recipe chosen by weight | Match |
| `FUN_14012cbf0` | A breeding animal in a pasture | Match |
| `FUN_140054600`, `FUN_1401270c0` | A herd's population cell, and its rest | Match |
| `FUN_1400fca20` | The emergency work lists | Match: the same nine stock lists in the same order |
| `FUN_140117060`, `FUN_140118340` | A birth's sex, and the birth timer | Match |
| `FUN_14011c550` | A new citizen's name, appearance and lifespan | Match |
| `FUN_140111d60` | Fetching herbs; the item is chosen before the walks, as in the port | Match |
| `FUN_140049a90`, `FUN_140049910` | A fire spreads: a cell near the building, then its odds | Match. The rows' range is not clamped at 0 in the original, which then reads outside the map; the port burns nothing there (a Deviation) |
| `FUN_14004ff60` | A nomad wanders to a cell near its camp | Match, with the first offset on y and the second on x |

## Not yet compared

The rest of the 51 ported functions. The larger ones still to read are `FUN_14008eea0` (happiness near a place), `FUN_1400c5b60` (a building
sequence), `FUN_140126420` (herds), `FUN_14010b8f0` (a wild animal), `FUN_1400a7490` (placed
descriptions), `FUN_1400ecd10` (a production cycle) and the start of a game (`FUN_1400a6780`,
`FUN_1400a62a0`, `FUN_1400a5fe0`).

## The order within an iteration

Each function above can match while the draws still come in another order, because the port
runs the entities' updates in another order. That is issue #22: the original visits the
entity buckets in turn and runs each entity's components together.

## Open questions

- The order of the animated meshes in `MeshInstance::Update`'s pass (the animation manager's
  list), which decides the order of the sound events' draws.
