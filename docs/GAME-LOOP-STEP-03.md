# Game loop step 03: calendar accumulator and simulation rate

This step follows the calendar update inside `Game/Game::Update`. Evidence is static Ghidra 12.1.4
pseudocode for the Steam x64 executable and typed reads of the executable's own constants. It is not
a live-game observation or a fresh REA session.

## Calendar state

`FUN_14009bfd0` (`0x14009bfd0`) allocates a small calendar state object at `Game/Game +0x128`:

| Offset | Initial value | Observed use |
|---:|---|---|
| `+0x00` | `1` | Displayed year; increments at the annual boundary. |
| `+0x08` | `Time(0)` | Elapsed simulation time within the current year. |
| `+0x10` | `Time(3600 seconds)` | Annual threshold. |

`DAT_14017895c` is `3600.0f`; `DAT_1401788b8` is `12.0f`. The toolbar's update path converts
elapsed time to a month index with `floor(elapsed / 3600 × 12)` and selects the corresponding
`Month0` through `Month11` string-table entry. This gives twelve equal months of **300 simulation
seconds each**. Year 1 starts at elapsed time zero.

## How speed affects the calendar

In `FUN_14009b850` (`0x14009b850`), the outer simulation count is zero while paused and otherwise
comes from the controller at `Game/Game +0x120`, offset `+0x34`. For every iteration, the same
frame delta is added to the calendar elapsed time. When elapsed time reaches 3,600 seconds the
year increments and the threshold is subtracted. The toolbar values 1, 2, 5 and 10 therefore scale
the rate of calendar time as well as repeating the manager update; see
[GAMEPLAY-SPEED-AND-TOOLBAR.md](GAMEPLAY-SPEED-AND-TOOLBAR.md).

At a steady 1x, a month lasts five real minutes and a year one hour. The corresponding month
durations are 150 seconds at 2x, 60 seconds at 5x and 30 seconds at 10x. A UI pause bypasses the
iteration block, so calendar time stops while the outer update continues to update UI and render
state.

## Port change

`GameplayLoop` now uses the four traced speed multipliers and advances its 12-month calendar at
300 simulation seconds per month. It starts in year 1 at Month0. This ports the clock and displayed
season/month label; production, citizens, weather and resource consumption still do not run.

## Evidence pointers

- `analysis/export/Application-steam-x64.exe/decomp.c`: `FUN_14009bfd0` initializes the calendar
  state; `FUN_14009b850` advances it and applies the speed count; `FUN_1400be210` computes the
  displayed month index.
- `tools/ghidra/readdata.py game-data/original/Application-steam-x64.exe f32 0x14017895c 0x1401788b8`
  reads `3600.0` and `12.0` from the executable.
- The toolbar speed values and event mapping are documented in
  [GAMEPLAY-SPEED-AND-TOOLBAR.md](GAMEPLAY-SPEED-AND-TOOLBAR.md).
