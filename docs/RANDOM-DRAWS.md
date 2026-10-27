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
| `FUN_140117940` | A sick citizen passes the disease on: one draw, then at most one citizen offered it | `tools/emu/disease_spread.py` |
| `FUN_1400ce640` | A walk's speed factor; a NaN speed is kept with no draw | `tools/emu/move_speed.py` |
| `FUN_14005be50`, `FUN_14005bdb0` | A trading post's row and the time to the next merchant | `tools/emu/trade_timer.py` |
| `FUN_14008eb70`, `FUN_14008e940` | Shuffling the residences, and a profession's workplaces (the same loop) | `tools/emu/shuffle.py` |
| `FUN_14011e940` | A production cycle's recipe, chosen by weight | `tools/emu/recipe_pick.py` |
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
| `FUN_1400581c0` | A tombstone's lifetime | Differed: the original draws it inside the death (`FUN_14011c2a0` → `FUN_1400473f0` → the entity constructor's setup), right after the grave's draw; the port drew it later in the update. Fixed (issue #23): `People::die` draws it and the burial carries it |
| `FUN_1400475c0`, `FUN_14008ec10` | A free grave and the cemetery search | Match, including `Rand(0)`, which draws nothing |
| `FUN_140048f70`, `FUN_1400484a0`, `FUN_1400491a0` | Blight: the chance, a crop that withers, the spread | Match |
| `FUN_14004db00` | An immigration wait | Match |
| `FUN_1401228d0` | A building being destroyed collapses | Match |
| `FUN_14012cbf0` | A breeding animal in a pasture | Match |
| `FUN_140054600`, `FUN_1401270c0` | A herd's population cell, and its rest | Match |
| `FUN_1400fca20` | The emergency work lists | Match: the same nine stock lists in the same order |
| `FUN_140117060`, `FUN_140118340` | A birth's sex, and the birth timer | Match |
| `FUN_14011c550` | A new citizen's name, appearance and lifespan | Match |
| `FUN_140111d60` | Fetching herbs; the item is chosen before the walks, as in the port | Match |
| `FUN_140049a90`, `FUN_140049910` | A fire spreads: a cell near the building, then its odds | Match. The rows' range is not clamped at 0 in the original, which then reads outside the map; the port burns nothing there (a Deviation) |
| `FUN_14004ff60` | A nomad wanders to a cell near its camp | Match, with the first offset on y and the second on x |
| `FUN_1400c5b60` | A builder's sequence at a build point | Match: `Rand(2)`, then the walk's speed |
| `FUN_14005cb80` | A trading post's merchants get names | Differed: `Rand(2)` of 0 sets the female flag of `FUN_1400aa130` (`sete` at `0x14005cbfe`); the port gave a man's name for 0. The name generator draws from the game's Random, so the draws after it differed too. Fixed |
| `FUN_14010b720` | A wild animal idles, and may turn | Match: the first `RandFloatTol` on y, the second on x |
| `FUN_1400486d0` | Blight kills an animal in a pasture | Match: up to 50 `Rand(count)`; the first animal no one hunts goes at once |
| `FUN_14011c060` | A citizen is offered a disease | Match: one `RandFloatTol` for how long it lasts |
| `FUN_140050b00` | A nomad joins the town as a citizen | Match: the age, then `Rand(1)` for education. `Rand(1)` is always 0, so a nomad never arrives educated, in the original too |
| `FUN_1400fa960` | Tending a field or an orchard | Match: `Rand(count)` for the cell or tree, after the same worker test |
| `FUN_14011f7e0` | A crop field enters state 9 and shuffles its cells | Match: `Rand(n)` for each position whenever n is above 0, so one cell still draws (unlike the workplace shuffles, which need two) |
| `FUN_14012bcb0` | A pasture leaves its product piles | Match: each pass takes a pile's worth off what was produced, rescans the points, skips a pile, clears anything else and makes one pile, whose `RandTol(2) × 90°` turn is drawn after the entity is made |
| `FUN_14005bee0` | A merchant's goods | Match: `Rand(merchants)`, then one `Rand(goods)` per lot over a list that does not shrink, with the same amounts and the 1.25 for ordered goods |
| `FUN_14004e540` | A band of immigrants arrives from a map edge | Match: `Rand(4)` for the edge, then one `Rand(n)` for the free coordinate (drawn first on edges 2 and 3), ten tries; then the band's size |
| `FUN_1400f3880` | A herbalist looks for a plant | Match: `RandTol` pairs until inside the circle; the original's column test can never hold, and the port keeps that |
| `FUN_1400f7840` | Tending a pasture | Match: `Rand(h)` for the row, then `Rand(w)` for the column |
| `FUN_1400fb210` | A trade work offer | Match: `Rand(points)` before each haul, in both branches |
| `FUN_1400f5450` | A market work offer | Match: two `Rand(point count)` over the market's own points and then its parts', at the same places as `market_point`; the same worker test (`max <= plans`) |
| `FUN_140113170` | The AI's candidate list | Match: a `Rand(count)` swap at each position, then the CRT `qsort` (`tools/emu/candidate_sort.py`) |
| `FUN_1400a7490` | The starting buildings from the start condition | Match: the entity is made first (only the model's setup can draw, when its variant is random), then one `Rand(variant count)` for each building with a model, as `place_structures` does after `construct` |
| `FUN_14010b8f0` | A wild animal's update: breeding, home, idle or a walk in its area | Match in every branch: `Rand(4)` only inside its area, rows drawn before columns, the first offset on y, a new area after 10 tries, `Rand(3) + 1` without ground, the idle time's `RandFloat` |
| `FUN_1400ecd10` | A production cycle (ConsumeProduce slot `+0x20`) | Match: the recipe pick (`FUN_14011e940`) before `Rand(slots)`; each haul (`FUN_1400c4c60`) makes the walk to the target before the walk back, as `Sim::haul` does |
| `FUN_14008eea0` | A happiness place near a cell, for loitering | Match: candidates in registry order, the same path-rank arguments (`cell`, then the use point) and fallback, `Rand(count)` only with candidates |
| `FUN_140126420`, `FUN_140126920` | A herd's animals are made, and a herd grows back | Differed: the original makes each animal (whose deer model draws its variant, `FUN_140129f90`) before the offsets and the heading. The port made it after them. Fixed |

## The new game, compared with the original's saves

`Save2` and `Save3` are saves the original made 63 and 70 iterations into new games (seeds
119484825 on Valleys and 907101598 on Mountains, Medium start, Fair climate). The port starts a
new game with the same seed and settings, runs to the same iteration (`--sim-continue`) and is
compared with `save_random_distance` and object by object. Two differences in the start were
found this way and fixed:

| Where | What differed | Result |
|---|---|---|
| An animal's lifespan, `FUN_14010c220` | The AnimalComponent's setup draws the lifespan (`RandFloatTol`) inside the entity constructor, right after the model's variant (the model comes first in every animal template). The port drew it at registration, after the offsets, the heading and the age, so every deer had another deer's age and lifespan. Livestock bought at a trading post got age 0, because its age is half the lifespan drawn at that point | Fixed: all 16 herds of `Save2` match field for field |
| The start structures, `FUN_1400a7490` | The barn and the house are built at once and hold their cells (occupant and type) before the deer population starts. Its capacity (`FUN_1400544b0`) leaves out a population cell with an occupant, so the herd whose block holds the town picks its target from 60 cells, where the port offered 64 | Fixed: the port marks their footprints right after placing them |

A second pass compared every walk, animal, herd, citizen and work action of the two saves
field by field. It found that a herd's deer alternate between male and female after a random
first (the flag is flipped after each, `0x1401267d9`, `0x140126cb0`), that an educated starting
citizen also has its school done (`+0x220`, `FUN_14011c550`), and four values the port's saves
wrote wrongly (the citizen's links to its CharacterComponent and StatusIconComponent, `+0x1d8`
and the home slot as the constructor `FUN_140119c40` leaves them, and PerformWork `+0x44`, −1
from `FUN_1400ceb80`). With those fixed, 170 of the 172 objects compared are identical; the two
left are the walks made after the audio's draws described below.

After these, every object type has the same count in both towns, and the random state is 29
draws behind `Save2`'s (25 behind `Save3`'s). The first walk that differs is the 16th citizen's,
the first one made in the second iteration (bucket 1 visits 15 citizens an iteration). Adding 11
draws at the end of the first iteration gives both later walks their original speeds, so the
original draws 11 more between the 15th citizen's walk and the 16th's. The weather's draws of
the first iteration come just before that point and match (`Save2`'s baseline and bias targets
are the port's to the last bit), so the 11 come after the environment's update. The original's
game loop (`FUN_14009b850`) then runs the stock manager, the post-iteration pass and the music
player (`FUN_1400a95b0`, which draws `Rand(n)` from the game's Random), and the audio scene
draws from the same Random. `--sim-continue` runs no audio, so these draws, which depend on
the frame rate and on when sounds end, are the expected remainder. The game itself now makes
them (see "The audio scene" below).

With those 11 draws added at the end of the first iteration, every walk of `Save2` has the
original's speed and path, and all 17 citizens are identical to the last bit at iteration 63:
position, heading, speed, action stack and animation clock. 75 of the 98 deer are identical too.
The other 23 are off by about 0.01 in position and a fraction of a degree in heading. Their
wander targets are drawn from the game's Random after the 18 draws still missing at that point,
which come from the audio as well and are spread over several iterations, so a single insertion
does not account for them.

To repeat the comparison: start a new game with `UNBANISHED_SEED` set to the save's seed (and
the same terrain, with the probe's `combo comboTerrain=…`), pause and save it with the capture
script, run `--sim-continue` up to the original save's iteration (its entity time divided by
16,666 µs), then run `save_random_distance` on the two saves. The temperature is not comparable
after a load: neither game saves its noise.

Four components draw in their setup (slot `+0x20`, run inside the entity constructor
`FUN_1400c8bf0`): the model's variant (`FUN_140129f90`), an animal's lifespan
(`FUN_14010c220`), a tombstone's lifetime (`FUN_1400581c0`) and `CreateNaturalResource`
(`FUN_1400cbb20`). Wherever the original makes an entity, these draws come at that point, in its
template's component order.

## The audio scene

`Audio::Scene::Update` (Runtime `0x1800e1f10`) takes the Random it is given: the game's
(`Game +0xd8`) in a game (`FUN_14009b850`), the menu's own (`+0x168`) in the main menu
(`FUN_1400a0bb0`). For each effect whose type is not paused, each sound of its timeline whose
start falls in the frame draws `Rand(variant count)`, skipped when there are no variants, and
then one number for the pitch (*observed*). Every UI sound (ButtonDown, ButtonUp, Alert,
Warning and the placement sounds) has one sound with one variant, so it draws 2. A click plays
ButtonDown on the press and ButtonUp on the release, so it draws 4.

The pause menu pauses types 2 to 4, and the UI type keeps running, so the clicks made in the
game menu and the Save window draw from the game's Random while the town waits. The draws
missing from the order-free pairs ([ITERATION-ORDER.md](ITERATION-ORDER.md)) fit this: 8 for
two clicks, and 22 to 25 for the clicks of the menu with presses that registered only in part
(*inferred*: the tool pressed each button up to four times, and the game reads input about
every two seconds in the emulated PC). The music player (`FUN_1400a95b0`) draws `Rand(n)`
when its track has ended, which explains one odd draw.

The port draws them from the game's Random in a game (`gameplay/audio.rs`), and from the
audio's own in the main menu, a multiplayer game, a recording and a replay. Test:
`a_click_in_the_pause_menu_draws_four_from_the_game_random`.

## The order within an iteration

Each function above can match while the draws still come in another order, because the port
runs the entities' updates in another order. That is issue #22: the original visits the
entity buckets in turn and runs each entity's components together.

## Open questions

- The sound events' draws have no pass of their own. The ModelComponent's every-iteration
  update (`FUN_140129e30`, bit `0x04`) calls `MeshInstance::Update` through the mesh's vtable,
  which fires the clip's events (*observed*). A citizen, an animal or a boat lists its
  ModelComponent before its CharacterComponent, so each one draws for its sounds just before its
  character runs; the port does the same. Their order between entities is the every-iteration
  set's (by address), step 3 of issue #22.
