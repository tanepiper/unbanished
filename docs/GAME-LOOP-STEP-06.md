# Game loop step 06: weather state persistence and status bar resources

This step records the environment controller's starting and serialized state, then decodes the
shipped Status Bar resource tree. Evidence combines static Ghidra 12.1.4 pseudocode for the Steam
x64 executable with compiled UI resources decoded by `tools/crs/types/ui.py`. The controller's
bindings and update behavior are traced in [GAME-LOOP-STEP-07.md](GAME-LOOP-STEP-07.md); no live
game's displayed values have been observed.

## Environment controller state

`FUN_140099430` initializes the controller's weather state to 0 and its separate level to 4. It
initializes the Perlin generator, clears the smoothed weather bias and current temperature, and sets
the last processed weather-slice index to -1. On the first update at slice 0, `FUN_140098ac0` makes
the first weather-state draw and adjusts the level: state 0 adds one, state 3 subtracts one, and the
result clamps to 0–5. This means 4 is the constructed starting value; the first processed weather
slice can change it immediately.

`FUN_140099160` serializes the weather state and level, current temperature and weather bias, the
Perlin seed state, the selected slice, and each present weather effect's `+0xc4` and `+0xc0` floats.
The controller therefore preserves this weather state across save/load instead of recomputing all
of it from the calendar on restore. Its active graphics and audio objects are reconnected by the
serialization path; the exact restore ordering remains a separate question.

`FUN_1400988f0` derives the renderer's rain input from the maximum `+0xc0` value among active
weather effect geometries. The level field at controller offset `+0x14` is not used by that
calculation. The level's downstream effect on rain, snow accumulation, or another UI/graphics
consumer is still unidentified.

## Shipped Status Bar tree

The decoded `Dialog/StatusBar_content` layout places six groups in its table:

1. `groupSeason`
2. `groupPopulation`
3. `groupWeather`
4. `tableStats`
5. `groupHealth`
6. `groupHappy`

`groupSeason` contains `textSeason` and `numberYear`. `groupWeather` contains a nested icon group,
`imageTemperature`, and `numberTemperature`. The icon group contains Sunny, Partly Sunny, Cloudy,
Rain, and Snow images. The resource files and sprite names confirm the intended visual states, but
do not tell which state corresponds to every controller threshold or when the icons are shown.

These resources expose more of the intended gameplay HUD than the current port's single date label.
`FUN_14009d0d0` installs the top-level Toolbar, EventNotify and AutoSave dialogs. The Status Bar
controller's `OnUpdateLayout` callback is identified in step 07; the direct Toolbar-to-StatusBar
resource handoff remains unproven.

## Evidence pointers

- `FUN_140099430` (`0x140099430`): environment controller defaults.
- `FUN_140099160` (`0x140099160`): controller save/load serialization.
- `FUN_140098ac0` (`0x140098ac0`): state and level transition at each weather slice.
- `FUN_1400988f0` (`0x1400988f0`): renderer weather values sourced from active geometry.
- `Dialog/StatusBar_content`, `groupSeason`, `groupWeather`, `groupWeatherIcons`,
  `numberTemperature`, `numberYear`, and the five `image{Sunny,PartlySunny,Cloudy,Rain,Snow}`
  resources decoded with `tools.crs.types.ui`.
- `analysis/environment-controller.txt` and `analysis/environment-nearby-functions.txt` are local
  Ghidra exports; their findings are summarized here rather than committing pseudocode.

## Next trace

Follow the `UtilityStatusBar` gameplay installation in step 07. Separately trace the environment
weather level through effect geometry and any consumer beyond the status icon. Keep the saved
controller state intact when connecting restore behavior.
