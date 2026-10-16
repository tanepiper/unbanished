# Tornado

Port: `crates/game/src/tornado.rs` (TornadoComponent), `Environment::update` (the spawn),
`apps/game/src/start/tornado.rs` (the entity). Test: `a_tornado_crosses_the_town`
(`apps/game/src/placement_tool.rs`).

## When one forms

The environment's update (`FUN_1400999b0`, step 6) adds the frame time to `+0x70`. While it
rains (state 0) above 34°F, and `Game +0xd0` bit 7 is clear (it is set with disasters off),
`FUN_1400ae760(+0x70, climate +0x108)` decides: `RandFloat < seconds / (years × 12 × 300 ×
odds)` on the game Random, the timer restarting. A tornado is the EnvironmentDescription's
template (`+0x20`, `Template\Tornado.rsc`): `FUN_1400c8bf0`, `FUN_1400c9d70`. The only other
way is the debug window (`FUN_140060680`).

## TornadoDescription

Serializer `FUN_1400596f0`: `+0x20` speed (3 cells a second), `+0x24` the ends' least
distance (64), `+0x30` a pass's reach (8), `+0x28` the time between passes (10 ms), `+0x34`
how far a pass throws its point (20).

## TornadoComponent

Vtable `0x140160ba0`; serializer `FUN_1400594e0` (the point it heads for `+0x30`, its index
`+0x50`, the line `+0x40`, the pass timer `+0x28`, the position `+0x58`).

**Setup** (slot `+0x20`, `FUN_140058f00`): four `Rand(map size)` (start x, y, end x, y) until
the ends are at least `+0x24` apart, at most a hundred tries. The line has 129 points: the
start's and end's ground (height 0), then `FUN_1400592e0(0, 128)`: the midpoint moves to the
left of the line by `RandFloat × distance / 4`, then each half the same (depth first). The
tornado stands at the start's ground point and heads for the line's first point. It posts the
event "A tornado has touched down near the town!" (type 30, severity 3, category 4), which
asks AutoSlow to slow the game.

**Update** (slot `+0x40`, `FUN_140058630`, every frame):

1. When its particle system has no particles left, the entity goes (`FUN_1400c8370`).
2. `FUN_1400ac6b0(10, 3)`: within 3 of its point, the next point at least 10 on
   (`FUN_1400ac590`); past the line's end, false.
3. False: the particle system stops forming particles (`FUN_140054080(0)`). True: a wrecking
   pass (below).
4. It moves `speed × dt` toward its point and stands on the terrain (`FUN_1400a48b0`, floors
   skipped); the particle system and its sound follow it.

**A wrecking pass** (`FUN_1400589a0`): the timer counts down the frame time; at 0 it adds
`+0x28` and:

- throws a point: `RandFloatTol × 180°` about the tornado, `RandFloat × +0x34` away (both on
  the game Random); the natural resource on that cell is hit;
- every entity whose model stands within `+0x30` of the point is hit (`FUN_140097e60`);
- every object of the cells within `+0x30` of the tornado is hit.

**A hit** (`FUN_140058d20`):

| Entity | What happens |
|---|---|
| a natural resource that grows (crops, trees) | unless its model is animating, its growth goes to 0 and it is removed (`FUN_14012bc50`, the falling clip) |
| a building (Build and Fire) | unless damaged already, damaged (`FUN_140123c80`, time 0) and turned to rubble (`FUN_140123ee0`) |
| an animal | removed (`FUN_1400c9ea0`) |
| a citizen | dies, code 5 (`FUN_14011c2a0`) |

## Port notes

- The tornado's particles (the Tornado particle class, `FUN_1400d7940`, drawn on the runtime's
  own Random) are not ported; the entity goes 32 s (their longest life) after it stops
  forming them.
- Its victims among the citizens die in the citizens' pass of the same iteration.
- The sound follows it; its volume does not follow the particle system's.
