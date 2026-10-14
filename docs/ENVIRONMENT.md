# Environment controller: temperature, weather and seasons

This note describes the environment controller as ported. It was built from the traces in
[GAME-LOOP-STEP-04.md](GAME-LOOP-STEP-04.md) and [GAME-LOOP-STEP-05.md](GAME-LOOP-STEP-05.md),
re-read against Ghidra 12.1.4 pseudocode of the Steam x64 build (1.0.7), with constants read
directly from the executable with `tools/ghidra/readdata.py`.

- **Port:** `crates/game/src/environment.rs`, with unit tests.
- **Driver:** the gameplay loop (`apps/game/src/gameplay.rs`).

No live game was observed. The values have not been compared with a running original.

## Where it runs (`FUN_14009b850`)

The gameplay update runs `speed` simulation iterations per frame (1, 2, 5 or 10; 0 while paused).
Each iteration gets the same frame delta. In order, each iteration:

1. updates entities and animation;
2. advances the calendar (`Game +0x128`: year count, time into the year, year length). When the
   time reaches the year length, the year increments and the length is subtracted;
3. calls `FUN_1400999b0`, the environment update;
4. calls the citizen manager and the other managers.

The port runs the calendar and environment steps of that loop. The other managers are not ported.

## Construction and binding

- `FUN_140099430` sets weather state 0 and level 4. It seeds the noise from two clock-seeded
  `Random()`s and sets the last slice to −1.
- `FUN_140099300` copies the climate's baseline (`+0x34`) into both the smoothed (`+0x2c`) and
  target (`+0x30`) baseline.
- The first update always lands on slice 0, which reseeds the noise from the game stream. So the
  clock-seeded noise is never used for a new game.

## Each update (`FUN_1400999b0`)

1. **Smoothing.** Both targets move with `cur = target·0.0009999871 + cur·0.999`
   (`DAT_1401787b4` and `DAT_140178848`). This applies to the baseline (`+0x2c`) and the
   weather-cutoff bias (`+0x44`).
2. **Temperature** (`FUN_140098ce0`):
   - `T = sin(((t − 300) / 3600) · 360 · 0.017453292) · climate[+0x38] + baseline`
     `+ noise(t / 180, 0) · climate[+0x3c]`;
   - the noise is `Utility::PerlinNoise(0.25, 4 octaves)`;
   - the coldest point is at month 10 (t = 3000 s) and the warmest at month 4.
3. **Renderer values** (`FUN_1400988f0`, `pc_userPosition`):
   - snow = `clamp(32 − T, 0, 1)`;
   - autumn = `clamp((month − 6) / 1.5)`;
   - leaf loss falls from 1 to 0 over months 0–0.5, stays 0 until 7.5, then rises at 2 per month;
   - rain is the largest intensity (`+0xc0`) among the active weather-effect geometries.
4. **Weather draw** (`FUN_140098ac0`). The slice is `floor(t / 3600 · 48)`, 75 s each. When it
   changes:
   - **At slices 0 and 24:**
     - `Rand()` twice; the noise is reseeded with `(second, first)` as `(seed_x, seed_y)`;
     - then 12 rounds of two `RandFloatTol()`. The first value of each pair averages into the
       bias and the second into the baseline;
     - baseline target = `avg · climate[+0x40] + climate[+0x34]`;
     - bias target = `avg · climate[+0x104]`.
   - **Then on every slice change:**
     - one `RandFloat()` picks the state: the first `i` in 0..2 with
       `sample < bias + month[i]`, otherwise 3. The month is `slice / 4`;
     - state 0 raises the level and state 3 lowers it, clamped to 0..5.
5. **Effects.** If weather effects are enabled (option byte `DAT_1401cbde0 + 0x16`) and the state
   is 0, slot 0 (Rain, `Environment/Rain.rsc`, with ambient rain audio) is active above 34°F, and
   slot 1 (Snow) at 34°F or below (`FUN_140099790`).
6. **Timed spawn.** An accumulator at `+0x70` adds the frame delta. When `T > 34`, the state is 0
   and game flag bit 7 is clear, `FUN_1400ae760` can spawn an entity from the environment template
   (`+0x108` timer settings). This is *inferred* to be the rain-time event (for example
   lightning). It has not been traced further.

`RandFloatTol` returns a value in (−1, 1]. So the annual baseline lies within
`climate[+0x34] ± climate[+0x40]`; Fair is 50 ± 40 °F. The bias lies within ±`climate[+0x104]`.
This corrects the one-sided range given in step 05.

## Status Bar link

The Status Bar ([UTILITY-WINDOWS.md](UTILITY-WINDOWS.md)) shows:

- **Temperature:** `+0x18` (`T`), converted to °C when "Show Celsius" is set;
- **Weather icon:** `+0x10` (the state). State 0 shows Snow at 32° or below, otherwise Rain.

## Port status and deviations

- **Ported:**
  - construction, smoothing, temperature, the renderer's snow, autumn and leaf values, the
    weather-state draw and the level;
  - the gameplay loop drives it once per simulation iteration and feeds `pc_userPosition` and the
    Status Bar from it.
- **Deviation: shared random stream.** The game stream (`Game +0xd8`) is `Random(seed)`, but in the
  original the new-game seeding consumes it first: structures, rocks, ore and trees, households,
  deer, crops and livestock (see [RESOURCE-SEEDING.md](RESOURCE-SEEDING.md)). Citizens and other
  managers draw from it during play too. Until those are ported, the port's environment draws start
  at a different stream position than the original's.
- **Not ported:**
  - the rain and snow effect geometry and its intensity (`+0xc0`), so rain stays 0;
  - the rain ambient emitter;
  - the timed spawn (`FUN_1400ae760`);
  - save/load of the controller state (`FUN_140099160`).
