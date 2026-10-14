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
month-indexed thresholds and updates a weather state. The three cutoffs are the same in the
shipped Mild, Fair and Harsh profiles; the fourth float in each monthly record is `1.0` and is not
read by this selector. The state advances through 0–3 by comparing one random float with the
cutoffs in order. State 0 also increments a separate level, while state 3 decreases it; that level
is clamped to 0–5.

The decoded `Environment/Environment.rsc` identifies its two slots as `Environment/Rain.rsc` with
`Audio/Effects/RainEffect.rsc:ambient`, and `Environment/Snow.rsc` with no ambient audio. In
`FUN_1400999b0`, either slot is active only when the global weather-enable flag is set and the
weather state is 0. It selects Rain when the controller's temperature is above 34 and Snow at 34 or
below. This maps the rain/snow effect slots to code conditions. The interpretation of the other
weather states and the separate 0–5 level still needs confirmation from their consumers.

`FUN_140099790` creates or removes the selected slot's global graphics object and, if present, its
entity/audio emitter. Thus the rain slot has a matching ambient emitter and the snow slot does not.

## Decoded climate cutoffs

`FUN_140098470` serializes four floats for each of twelve months. `FUN_140098ac0` uses the first
three as cumulative cutoffs. For cutoffs `a, b, c`, the probabilities of states 0–3 are
`a`, `b-a`, `c-b`, and `1-c`. The fourth float is 1.0 in all shipped records and does not
participate in this selection.

| Month | Cutoffs | State probabilities 0 / 1 / 2 / 3 |
|---:|---|---|
| 0 | 0.5, 0.7, 0.9 | 50% / 20% / 20% / 10% |
| 1 | 0.4, 0.6, 0.6 | 40% / 20% / 0% / 40% |
| 2–3 | 0.4, 0.5, 0.7 | 40% / 10% / 20% / 30% |
| 4 | 0.3, 0.3, 0.6 | 30% / 0% / 30% / 40% |
| 5 | 0.2, 0.4, 0.6 | 20% / 20% / 20% / 40% |
| 6 | 0.3, 0.5, 0.7 | 30% / 20% / 20% / 30% |
| 7 | 0.4, 0.6, 0.8 | 40% / 20% / 20% / 20% |
| 8 | 0.5, 0.6, 0.9 | 50% / 10% / 30% / 10% |
| 9 | 0.6, 0.6, 0.9 | 60% / 0% / 30% / 10% |
| 10 | 0.7, 0.8, 0.9 | 70% / 10% / 10% / 10% |
| 11 | 0.6, 0.7, 0.9 | 60% / 10% / 20% / 10% |

The profiles differ in other serialized settings: Mild `[50, 30, 2, 20]`, Fair `[50, 30, 5, 40]`,
Harsh `[50, 30, 15, 50]`, followed by `[0.1, 100, 1]`, `[0.4, 100, 1]` and `[0.6, 100, 1]`
respectively. The consumer traces show these values affect the temperature/noise and weather state,
but several field meanings are not established, so they remain recorded in wire order.

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

- Trace the unlabelled climate settings into their exact temperature, intensity and persistence
  consumers.
- Follow how the 0–5 precipitation level affects snow accumulation and the terrain/model shaders.
- Follow the climate state into the ambient-light and surface/material parameters, then port the
  complete weather transition without replacing its random state with a visual guess.

## Evidence pointers

- `analysis/export/Application-steam-x64.exe/decomp.c`: `FUN_14009bfd0`, `FUN_14009b850`,
  `FUN_140099300`, `FUN_1400999b0`, `FUN_140098ac0`, `FUN_140099790`, `FUN_140098f90`,
  `FUN_140099ec0` and `FUN_140098da0`.
- `tools/ghidra/readdata.py game-data/original/Application-steam-x64.exe f32 0x1401788e4` reads
  `48.0` from the executable.
- `crates/formats/src/game.rs` decodes the twelve `EnvironmentMonth` records and the environment
  weather-slot references, plus the `ClimateGenerator` wire record.
- `cargo run -p banished-formats --bin climate_info -- game-data/extracted/Game/Climate/{Mild,Fair,Harsh}_resource.crs`
  prints the month cutoffs and remaining settings; `environment_info` prints the effect/audio slots.
- [GAME-LOOP-STEP-03.md](GAME-LOOP-STEP-03.md) records the calendar state and speed scaling.
