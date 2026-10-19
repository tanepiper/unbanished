# Environment controller: temperature, weather and seasons

This note describes the environment controller as ported. It was built from the traces in
[GAME-LOOP-STEP-04.md](GAME-LOOP-STEP-04.md) and [GAME-LOOP-STEP-05.md](GAME-LOOP-STEP-05.md),
re-read against Ghidra 12.1.4 pseudocode of the Steam x64 build (1.0.7), with constants read
directly from the executable with `tools/ghidra/readdata.py`.

- **Port:** `crates/game/src/nature/environment.rs`, with unit tests.
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

The port now runs the scheduled natural-resource/unique-animation phase, integer calendar and
environment in this order. Other managers remain unported. Step 19's bounded original-loop
emulation confirms the order and corrects the previous Rust driver's calendar/environment-first
sequence; see [GAME-LOOP-STEP-19.md](GAME-LOOP-STEP-19.md).

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
5. **Effects.** If weather effects are enabled (option byte `DAT_1401cbde0 + 0x16`, on by
   default) and the state is 0, slot 0 (Rain, `Environment/Rain.rsc`, with ambient rain audio)
   is wanted above 34°F, and slot 1 (Snow) at 34°F or below (`FUN_140099790`, called for both
   slots after the state draw):
   - a wanted slot without a geometry gets one (`FUN_1400d93e0`, intensity `+0xc0` = 0,
     direction `+0xc4` = +1) and sets the state to 0; with an ambient entity in the weather list
     (Rain only) it also registers that entity's emitter at the origin at volume 0;
   - an unwanted slot with a geometry gets direction −1 and **forces the state to 3**. So a
     temperature crossing 34°F ends precipitation until the next slice's draw;
   - at the start of each update (before step 1), every geometry's update (`FUN_1400d8330`)
     moves its intensity by `dt / fade × direction` (fade = `Weather +0x5c`, 20 s for both),
     clamped to 0..1, and the emitter's first slot volume follows. At 0 (or with the option
     off) the geometry and its emitter entity are removed. The renderer's rain value is the
     largest intensity (`FUN_1400988f0`).
6. **Tornado.** An accumulator at `+0x70` adds the frame delta. When `T > 34`, the state is 0
   and game flag bit 7 is clear, `FUN_1400ae760(+0x70, climate +0x108)` can spawn the
   environment template's entity, the tornado ([TORNADO.md](TORNADO.md)).

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
- **Ported:** the effect slots' intensity, direction and state forcing
  (`Environment::effects`, `fade_times` from the `Weather` descriptions), the renderer's rain
  value, and the rain ambient emitter (`apps/game/src/ambience.rs`).
- **Ported:** the precipitation (`apps/game/src/weather.rs`, `particle_gfx::WeatherRenderer`).
  - **Build** (`WeatherGeometry::Build`, `FUN_1400d9450`). The geometry gets `+0x28` particles
    from its own Random. Each starts in a cube of `+0x48`, drawn z, y, x × `RandFloatTol` × size
    / 2. Its velocity is the normalized `+0x2c` × (`RandFloatTol × +0x3c + +0x38`). Its last
    position is the start minus the velocity × 0.016.
  - **Update** (`FUN_1400d8330`, by the game time). Each particle moves and is wrapped back
    into the cube, its last position following. The intensity fades over `+0x5c` seconds; the
    port takes it from the environment's effect, which fades the same way.
  - **Draw** (`FUN_1400d85f0`). The cube is drawn once per tile of the 5 × 5 × 5 around the
    camera that is in the view. Each particle is a quad `p + R − U`, `p + R + U`, `q − R + U`,
    `q − R − U`, with uv `(0, 0)` to `(0, 1)` (indices `0, 1, 2, 0, 2, 3`):
    - snow (`+0x40` off) takes the camera's right and up × `+0x44` / 2 at its position;
    - rain takes a streak from its last position `q` to this one `p`, `+0x44` wide.
  - **WeatherMaterial.** The alpha is `tex.a × intensity`, faded by the depth between
    `+0x58`..`+0x54` and `+0x50`..`+0x4c` (rain 1–2 and 16–20, snow 2–5 and 25–30).
  - **Deviation.** The rain streak's sides are the cross of its velocity with the camera's
    forward axis. The original builds them from two camera planes through `Vector3::Cross`, but
    which planes those are is not traced.
**Weather lighting** (`FUN_140098f90`, with ShowWeather). After the month blend, each running
effect's Weather record pulls the lighting towards its own by the effect's intensity
(`FUN_140099ec0` with the angles flag clear). The record holds `+0x60` ambient, `+0x64` sun and
`+0x68` fog colours, and `+0x6c`, `+0x70`, `+0x74` fog start, end and most. The sun's angles
`+0x78`, `+0x7c` are not blended. Port: `Lighting::toward_weather`.

- **Not ported:** save/load of the controller state (`FUN_140099160`).
