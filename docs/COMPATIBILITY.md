# Compatibility between versions

Un-Banished must play exactly like Banished 1.0.7. Every new version must play exactly like the
one before it. This document describes the rule and the tools that check it.

## The rule

A change may make the engine faster, smaller or cleaner. It may not change the game's results:
the simulation, the random numbers, what a save contains, or what is drawn. "Not change" means
identical, bit for bit.

When a check finds a difference:

1. **Find the exact cause.** The difference can come from the change, or from the test itself
   (see [The save-timing trap](#the-save-timing-trap)). Check the old build against itself
   first.
2. **If it can be fixed, fix it.** For example, keep the original order of operations, or
   compute the same values another way.
3. **If it cannot be fixed, do not ship the change.** Open an issue that states the problem,
   the observed outcome, the suggested change and its impact on saves, behaviour and
   compatibility. The community votes on it.

## Tools

All of them run headless, with a fixed seed and a scratch settings file and save folder, so
they never touch the player's own files. They need `game-data/` and the test saves in
`save-game/` (both local and git-ignored).

| Tool | What it does |
|---|---|
| `tools/compat/build.sh REV` | Builds any revision (tag, branch, commit) in a clean, temporary git worktree; prints the binary's path (`target/compat/bin/banished-<sha>`) |
| `tools/compat/scenarios.sh BINARY OUT [SCENARIO…]` | Plays a fixed set of scenarios with one binary and keeps each save, frame and log |
| `tools/compat/compare.sh BASE NEW [SCENARIO…]` | Builds both (or takes binaries), and checks: BASE against itself, NEW against BASE, and both continuing BASE's saves |
| `tools/triage/same-run.sh BASE_BIN NEW_BIN SAVE` | A quick check of two binaries on one save |
| `tools/triage/determinism.sh` | Two identical new-game runs must draw identical frames |
| `savegame_same`, `savegame_fields_diff` (examples in `unbanished-formats`) | Whether two saves hold the same game state, and if not, which objects and fields differ |
| `tools/compat/replay.sh BINARY [SCENARIO…]` | Records the scenarios that load a save (`--record`), then replays each recording from its save without a window (`unbanished --replay`); every recorded digest must match |
| `tools/compat/lockstep.sh BINARY` | Plays the `actions` recording split between players joined by a simulated network with delay, jitter and busy computers (`unbanished --lockstep-test`); every setting must keep the players identical, and a scrambled player must be found and resynced |
| `tools/compat/network.sh BINARY [RELAY]` | Starts a relay on this computer and runs several game processes as the players of one room (`unbanished --net-test`); their digests must be identical |
| `tools/compat/multiplayer.sh BINARY [RELAY] [SCENARIO…]` | The Multiplayer window from end to end: headless games kept to the clock (`--realtime`) host a room and join it by typing its code (`--text`). Scenarios `two` and `leave` (a guest quits and the others play on) |
| `unbanished --sim-digest SAVE [UPDATES] [EVERY]` | Prints digests of the game state while a save runs; the browser build's `?digest=` prints the same, to show that native and browser builds simulate identically ([CROSS-PLATFORM.md](CROSS-PLATFORM.md)) |

```bash
tools/compat/compare.sh v0.2.0 HEAD                    # every scenario
tools/compat/compare.sh HEAD~1 HEAD new-game large-town  # a subset
tools/compat/replay.sh HEAD                             # record and replay
tools/compat/lockstep.sh HEAD                           # lockstep over a simulated network
tools/compat/network.sh HEAD                            # several games through a real relay
tools/compat/multiplayer.sh HEAD                        # the Multiplayer window, two and leave
```

### Scenarios

| Scenario | What it plays |
|---|---|
| `new-game` | A new game (seed 1234, Medium), 25 s at speed 5 |
| `small-town` | `save-game/Save0.sav` (29 citizens), 30 s at speed 1 |
| `medium-town` | `save-game/Save83.sav` (116 citizens), 30 s at speed 5 |
| `large-town` | `save-game/Auto-1.sav` (1,024 citizens, 28,000 entities, made in the original in 2015), 40 s at speed 10 |
| `actions` | `save-game/Save83.sav`, 30 s at speed 5, with a player's actions through the interface: worker buttons, placing a turned house, a road drag, clearing trees, demolishing by drag, the Path tool, a random town name and a rename, pause and play. It needs `--mouse-down` / `--mouse-up` (0.3.0 and later). |
| `continue:PATH` | A save written by another version, 25 s at speed 5 (backward compatibility) |

Each scenario pauses, saves and captures a frame at the end. A comparison passes only when every
save holds the same game state and every frame is the same.

### Limits

- Builds before 0.2.0 do not have the `speed N` capture command (they run at speed 1). Their
  headless saves can land on a different frame from run to run. Compare such builds only
  with themselves.
- Builds before 0.3.0 ignore `--mouse-down` and `--mouse-up`, so they skip the `actions`
  scenario's drags and its game comes out different. Compare `actions` only between 0.3.0 and
  later builds (for example, from 6416b7a); name the other scenarios when the base is older:
  `tools/compat/compare.sh v0.2.0 HEAD new-game small-town medium-town large-town`.
- **Several sessions at once.** Other sessions may run the same tools in the same checkout at
  the same time. Each run keeps its results in a folder named by the binary's file name and a
  short hash of its full path (`tools/compat/run-name.sh`), so two binaries with the same file
  name in different folders do not share one. `build.sh` builds a revision once: a second
  session that asks for the same revision waits for the first. Before this, two sessions that
  each tested a binary called `unbanished-haul` wrote into one folder. Each run deleted the
  other's saves, and the report showed "NO SAVE" or "missing" for scenarios that had played
  correctly. When a report shows those, look for another comparison running
  (`ps aux | grep compare.sh`) before looking for a crash.
- The tools compare Un-Banished with Un-Banished. Whether the original game loads a save written
  by Un-Banished is not verified yet.
- Timings in the scenarios are capture time, which runs faster than real time; the results
  do not depend on the machine's speed.
- **Frames can differ from run to run, even with the same build.** On the large town, and on
  the medium town at the highest video settings, a few frames come out in one of two versions:
  a few hundred pixels differ, mostly in the water's reflection of trees. The saves are always
  identical. The cause is not known yet. When a frame differs, run both builds again: a change
  is accepted when the new build's frames match one of the old build's versions.

## Changes meant to change the picture

A change that fixes the picture (to match the original game better) is allowed to change
frames, never saves. `compare.sh` then reports "NOT COMPATIBLE" because of the frames.
[Changing what is drawn](develop/VISUAL-CHANGES.md) explains how to show that every changed
pixel is one the change was meant to change.

## The save-timing trap

The first large-town comparison of the 0.2.0 performance work showed different saves: the
growth values of 835 trees differed. The cause was not the change. Headless captures wrote the
save after the game had drawn the save's preview image, and reading that image back from the
GPU took a varying number of frames. The game was paused at that point, but the growth smoothing
runs even when paused (as in the original, `FUN_1400c98a0`), so every extra frame moved the
values. Headless captures now wait for the GPU readback (`Renderer::blocking_readback`), and a
save lands on the frame it was asked for. Two runs of the same build agree, and so do the builds
before and after the performance work.

The large town's frame can still differ in a few pixels between two runs of the same build, with
an identical game state. During the player-command change (0.3.0) the build before the change
differed from itself once (a level difference of 1 of 255, in under 0.001% of the frame), and
once the continuation of a large-town save differed by up to 6 between the builds; run again,
both matched exactly. The cause is in the drawing, not the simulation, and is not found yet
([issue #16](https://github.com/tanepiper/unbanished/issues/16)). A frame-only difference in `large-town` should be repeated, and checked against
the old build compared with itself, before it is blamed on a change.
