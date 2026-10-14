# Game loop step 05: climate temperature and weather bias

This step follows the climate resource values into the environment controller's temperature and
weather-state calculations. Evidence is static Ghidra 12.1.4 decompilation of the Steam x64
executable and decoded shipped `ClimateGenerator` resources; it is not a live-game observation.
Binary identity and repeatable export commands are in [GAMEPLAY.md](GAMEPLAY.md).

## Temperature update

`FUN_140098470` serializes the climate fields used here at offsets `+0x34` through `+0x40`, followed
by twelve four-float month records at `+0x44`, and three more floats at `+0x104` through `+0x10c`.
The update path gives the first four fields concrete consumers:

| Climate offset | Mild / Fair / Harsh | Observed consumer |
|---|---:|---|
| `+0x34` | 50 / 50 / 50 | Baseline temperature added to the annual random target |
| `+0x38` | 30 / 30 / 30 | Amplitude of the yearly sine curve |
| `+0x3c` | 2 / 5 / 15 | Amplitude of Perlin temperature noise |
| `+0x40` | 20 / 40 / 50 | Range multiplied by an annual random average and added to the baseline |

At weather-slice indices 0 and 24, `FUN_140098ac0` reinitializes the Perlin seed and samples two
sets of twelve uniform random values. Each set is summed after multiplying each sample by `1/12`.
The first average selects an annual target in
`[climate[+0x34], climate[+0x34] + climate[+0x40]]`; the second is multiplied by the climate's
`+0x104` value to select that year's weather-cutoff bias. The same random generator is shared with
the wider game, so these equations do not by themselves reproduce the game's random sequence.

During each environment update, the current annual target approaches its selected value by a
`0.001` interpolation weight. `FUN_140098ce0` then computes the temperature from that smoothed
baseline, a yearly sine with the `+0x38` amplitude, and Perlin noise sampled at `elapsed / 180`
scaled by the `+0x3c` amplitude. The sine phase is `(elapsed - 300) / 3600 * 2π`; its implementation
is the math routine at `0x140137160`.

The environment selects its Rain slot above temperature 34 and its Snow slot at 34 or below. This
effect-selection threshold is distinct from the renderer's snow-coverage input, which is
`clamp(32 - temperature, 0, 1)`: visual snow coverage begins below 32.

## Weather-state bias

The three monthly values at climate offset `+0x44` are cumulative cutoffs. Before the annual bias,
their probability differences give the state distribution recorded in
[GAME-LOOP-STEP-04.md](GAME-LOOP-STEP-04.md). The climate's `+0x104` field (0.1 / 0.4 / 0.6 for
Mild / Fair / Harsh) scales a second annual random average. The controller smooths this target into
its current bias with the same `0.001` weight. `FUN_140098ac0` adds that current bias to each monthly
cutoff before comparing its random sample, so the table in step 04 is the unshifted distribution,
not the final per-year probability. Cutoffs can exceed one, making the first state certain for that
comparison.

The other trailing climate values (`+0x108` and `+0x10c`, 100 and 1 in all shipped profiles) have
not been connected to a consumer yet. Rain geometry intensity, accumulation, and the full weather
state interpretation remain open; no weather simulation is ported from this trace.

## Evidence pointers

- `FUN_140098470` (`0x140098470`): climate field serialization.
- `FUN_140098ac0` (`0x140098ac0`): annual seed/targets and the biased monthly weather-state draw.
- `FUN_140098ce0` (`0x140098ce0`): smoothed seasonal temperature plus Perlin noise.
- `FUN_1400999b0` (`0x1400999b0`): controller smoothing and the 34-degree Rain/Snow slot choice.
- `FUN_1400988f0` (`0x1400988f0`): the separate temperature-driven snow-coverage value.
- `FUN_140137160` (`0x140137160`): sine implementation used for the seasonal phase.
- `analysis/environment-climate-followup.txt`, `analysis/environment-climate-records.txt`, and
  `analysis/environment-math.txt` are local Ghidra exports; only the summarized findings are
  committed.

## Next trace

Trace how the 0–5 weather level and the remaining climate fields feed rain/snow geometry, ground
accumulation, and ambient lighting. Port temperature only after the engine-compatible random and
Perlin sequence can be represented without substituting fresh noise.
