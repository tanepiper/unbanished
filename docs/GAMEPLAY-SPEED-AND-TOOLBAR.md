# Gameplay speed controls and toolbar dispatch

This step follows the packaged toolbar's speed controls into the per-frame loop count. Evidence is
static Ghidra 12.1.4 pseudocode for the Steam x64 executable plus decoded `data0.pkg` UI resources;
the current Ghidra provider is not configured, so this is not a fresh REA session or a live-game
observation.

## Speed values and loop behavior

`FUN_1400bede0` (`0x1400bede0`) builds the toolbar controller's speed array at `+0xf8`. Its four
values, in order, are **1, 2, 5 and 10**. The selected index is stored at `+0xf4`, and the count at
`+0x100`. The constructor starts at index zero and writes the selected value to the game controller
at `Game/Game +0x34`.

`FUN_1400be680` (`0x1400be680`) finds the current value in that array, applies the button's `-1` or
`+1` change, clamps the index to the available range and writes the selected value back to
`Game/Game +0x34`. `FUN_14009b850` uses that field as the number of simulation iterations in its
outer update. Each iteration receives the same frame delta, so the speed control repeats the
simulation update; it does not select a different calendar duration in the observed code path.

The toolbar event handler `FUN_1400be770` maps item type 6 to slower, type 9 to faster, type 7 to
pause (`+0x30 = 1`) and type 8 to resume (`+0x30 = 0`). `FUN_1400be210` refreshes the enabled state
of the slow/fast and pause/play controls from the selected speed and pause byte, then passes the
current speed to the speed text element as parameter zero. The exact rendered formatting literal
is not resolved by this pseudocode export.

`FUN_1400be4a0` runs every update and sets each build entry (type 1) usable only while the
town has fewer sites of its template (the stock manager's count at `+0x160`: placed, not
destroyed) than the template's ToolbarDescription `_maxInstances` (`+0x60`; below zero means no
limit). The Town Hall's is 1, so its button greys out once one is placed. Town Info entries
(type 4) are usable once the town has at least one of their template. A disabled entry ignores
both clicks and its hotkey. The port does the same in `Toolbar::refresh`, called from
`Gameplay::refresh_toolbar` on every update.

## Packaged controls

The decoded records in `data0.pkg` associate the control kinds with these resources:

| Resource | Kind | Parent | Tooltip key |
|---|---:|---|---|
| `Game/Toolbar_slow.crs` | 6 | `Game/Toolbar.rsc:time` | `SlowTip` |
| `Game/Toolbar_pause.crs` | 7 | `Game/Toolbar.rsc:time` | `PauseTip` |
| `Game/Toolbar_play.crs` | 8 | `Game/Toolbar.rsc:time` | `PlayTip` |
| `Game/Toolbar_fast.crs` | 9 | `Game/Toolbar.rsc:time` | `FastTip` |
| `Game/Toolbar_speed.crs` | 10 | `Game/Toolbar.rsc:time` | `SpeedTip` |

The string table describes these as slowing down, pausing, resuming and speeding up the simulation;
`SpeedTip` says it shows the current simulation speed. These labels agree with the event handler.

## Port boundary

The port now uses the original 1x, 2x, 5x and 10x levels. The calendar accumulator and year/month
transitions have since been traced and ported; see [GAME-LOOP-STEP-03.md](GAME-LOOP-STEP-03.md).
Production and entity systems have not yet been connected to those simulation iterations.

## Evidence pointers

- `analysis/export/Application-steam-x64.exe/decomp.c`: `FUN_1400bede0` (`0x1400bede0`) constructs
  the speed array; `FUN_1400be680` (`0x1400be680`) changes the selected speed;
  `FUN_1400be770` (`0x1400be770`) dispatches toolbar clicks; `FUN_1400be210` (`0x1400be210`)
  refreshes toolbar state and the speed text; `FUN_14009b850` (`0x14009b850`) consumes the loop
  count.
- `data0.pkg`: `Game/Toolbar_{slow,pause,play,fast,speed}.crs` and
  `Dialog/StringTable_toolBar`.
- [GAME-LOOP-STEP-02.md](GAME-LOOP-STEP-02.md) traces controller and UI startup.
