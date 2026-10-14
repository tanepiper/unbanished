# Game loop step 04: environment and weather cadence

This step follows the active calendar into the environment controller. Evidence is static Ghidra
12.1.4 pseudocode for the Steam x64 executable and decoded resource schemas. It is not a live-game
observation or a fresh REA session.

## Calendar drives environment updates

`FUN_14009bfd0` initializes the environment controller at `Game/Game +0x130` by calling
`FUN_140099300` with `Environment/Environment.rsc` and the selected `ClimateGenerator`. In each
simulation iteration, `FUN_14009b850` calls `FUN_1400999b0` with the same frame delta as the calendar
and entity manager. The controller reads the elapsed year time from the calendar state at
`Game/Game +0x128`.

`FUN_1400999b0` converts that time to one of **48 annual weather slices** with
`floor(elapsed / 3600 × 48)`. `DAT_1401788e4` is `48.0`, so each slice spans 75 simulation seconds.
When the slice index changes, it calls `FUN_140098ac0`, which samples the selected climate's
month-indexed threshold data and updates an environment state. That state is bounded to 0 through
5 and influences which of the environment's two weather-effect slots are active. The pseudocode
shows random selection and state transitions; it does not name the states as rain, snow or another
specific phenomenon.

`FUN_140099790` creates or removes the selected weather slot's global graphics object and, when the
resource supplies an ambient-audio description, registers or removes its entity/audio emitter.
This ties the environment controller to both visible effects and sound, but it does not identify
which slot is rain or snow without examining the shipped records and their state conditions.

## Monthly lighting is continuous

The calendar month is `floor(elapsed / 3600 × 12)`. In
`FUN_140098f90` (`0x140098f90`), the controller interpolates from that month's
`EnvironmentMonth` to the next one using `(elapsed - month × 300) / 300`. The wrapped final month
blends into month zero. `FUN_140099ec0` interpolates the packed RGB colors and scalar lighting
fields; sun yaw and pitch are also blended before the sun direction is built. `FUN_140098da0`
applies the resulting fog and sunlight to the render context.

The port now interpolates adjacent month records each frame instead of switching lighting only at
month boundaries. The implementation is in `Lighting::between_months` and the `GameplayLoop`
update. The stochastic climate state, effect-slot creation and ambient emitters remain unported.

## What remains to identify

- Decode the three month-indexed climate thresholds and the additional per-month climate value in
  each climate record.
- Map the two environment effect slots and their activation conditions to the packaged Rain/Snow
  resources and ambient audio.
- Follow the climate state into the ambient-light and surface/material parameters, then port the
  complete weather transition without replacing its random state with a visual guess.

## Evidence pointers

- `analysis/export/Application-steam-x64.exe/decomp.c`: `FUN_14009bfd0`, `FUN_14009b850`,
  `FUN_140099300`, `FUN_1400999b0`, `FUN_140098ac0`, `FUN_140099790`, `FUN_140098f90`,
  `FUN_140099ec0` and `FUN_140098da0`.
- `tools/ghidra/readdata.py game-data/original/Application-steam-x64.exe f32 0x1401788e4` reads
  `48.0` from the executable.
- `crates/formats/src/game.rs` decodes the twelve `EnvironmentMonth` records and the environment
  weather-slot references.
- [GAME-LOOP-STEP-03.md](GAME-LOOP-STEP-03.md) records the calendar state and speed scaling.
