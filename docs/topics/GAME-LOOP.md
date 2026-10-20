# The game loop

This page explains how Banished runs from frame to frame: the engine loops, the simulation step,
the game speeds, and how the work of thousands of objects is spread over frames. The engine code
is in `apps/game/src/loops.rs` and `apps/game/src/gameplay.rs`; the rules it calls are in
[unbanished-game](../../crates/game/README.md).

## Engine loops

The engine runs one *loop* at a time. Each loop is one screen of the program:

1. the startup loader (the game's global data);
2. the Shining Rock splash screen;
3. the loading screen;
4. the main menu, with its music and a 3D town scene;
5. the game itself (`GameplayLoop`).

Between two loops, a loader loop builds the next one in small steps, behind a fade. In the
original some of this work runs on threads. The browser cannot block a frame, so Un-Banished does
the work in steps, and on the desktop it decodes data on all CPU cores. A loop owns a screen
with its dialogs, for example the main menu or the toolbar.

[NEW-GAME-FLOW.md](../NEW-GAME-FLOW.md) follows the path from the main menu's New button into the
game, and [MENU-SCENE.md](../MENU-SCENE.md) describes the menu loop.

## One frame of the game

The game loop's update runs a number of *simulation iterations*, then the work
that happens once per frame. The order was confirmed by running the original function in the
emulator:

1. For each iteration:
   1. the map, then the animations;
   2. the entity manager: the components that update every iteration (for example, characters
      walking), then the scheduled buckets (below), then the removal of deleted objects;
   3. the calendar;
   4. the environment (temperature and weather), then the town's managers.
2. After the iterations: the follow-up pass (for example, the smooth growth of trees), the
   camera, the interface and the sound.

The order matters. For example, trees see the temperature of the previous iteration, and their
random draws come before that iteration's weather draws.

## Game speed and pause

The toolbar's speeds are 1, 2, 5 and 10. The speed is the number of simulation iterations in
each frame, so at speed 10 the simulation does ten steps per drawn frame. Pause gives zero
iterations, but the follow-up pass still runs, so growing trees still move smoothly.

## Scheduled buckets

A large town has tens of thousands of objects: trees, resources, buildings, citizens. Updating
all of them in every iteration would be slow. So the entity manager puts objects into five
*buckets*, and each iteration updates only a few objects from each bucket:

| Bucket | Objects per iteration |
|---|---|
| 0 | 50 |
| 1 | 15 |
| 2 | 8 |
| 3 | 5 |
| 4 | 2 |

Each object receives the time since its own last update, so no time is lost. The order of the
buckets, and the position in each, are part of the save. `unbanished_core::scheduler` reproduces
this.

## Time

The engine counts time in integer microseconds. A year is 3,600 seconds of game time at speed 1,
and it has twelve months. [Exact maths and random numbers](EXACT-MATHS.md#game-time) explains the
conversions.

## Headless runs

The same loops run without a window. A headless capture runs the game at 30 frames per second of
game time, as fast as the computer can, and draws a frame only when asked. This is how the tests,
the screenshots and the videos in this documentation are made. See
[Verify](../develop/guides/VERIFY.md) and [Recording videos](../develop/VIDEOS.md).

## Read more

- [GAME-LOOP-CHECKLIST.md](../GAME-LOOP-CHECKLIST.md): every traced step of the loop.
- [GAME-LOOP-STEP-18.md](../GAME-LOOP-STEP-18.md): the buckets and their quotas.
- [GAME-LOOP-STEP-19.md](../GAME-LOOP-STEP-19.md): time units and the order of one iteration.
- [GAMEPLAY-LOOP-FOUNDATIONS.md](../GAMEPLAY-LOOP-FOUNDATIONS.md): the game's start-up.
- [PERFORMANCE.md](../PERFORMANCE.md): how long each part of a frame takes.
