# Compatibility between versions

Un-Banished must play exactly like Banished 1.0.7, and every new version must play exactly like
the one before it. This document describes the rule and the tools that check it.

## The rule

A change may make the engine faster, smaller or cleaner. It may not change the game's results:
the simulation, the random numbers, what a save contains, or what is drawn. "Not change" means
identical, bit for bit, not "close enough".

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
| `savegame_same`, `savegame_fields_diff` (examples in `banished-formats`) | Whether two saves hold the same game state, and if not, which objects and fields differ |

```bash
tools/compat/compare.sh v0.2.0 HEAD                    # every scenario
tools/compat/compare.sh HEAD~1 HEAD new-game large-town  # a subset
```

### Scenarios

| Scenario | What it plays |
|---|---|
| `new-game` | A new game (seed 1234, Medium), 25 s at speed 5 |
| `small-town` | `save-game/Save0.sav` (29 citizens), 30 s at speed 1 |
| `medium-town` | `save-game/Save83.sav` (116 citizens), 30 s at speed 5 |
| `large-town` | `save-game/Auto-1.sav` (1,024 citizens, 28,000 entities, made in the original in 2015), 40 s at speed 10 |
| `continue:PATH` | A save written by another version, 25 s at speed 5 (backward compatibility) |

Each scenario pauses, saves and captures a frame at the end. A comparison passes only when every
save holds the same game state and every frame is the same.

### Limits

- Builds before 0.2.0 do not have the `speed N` capture command (they run at speed 1), and
  their headless saves can land on a different frame from run to run. Compare such builds only
  with themselves.
- The tools compare Un-Banished with Un-Banished. Whether the original game loads a save written
  by Un-Banished is not verified yet.
- Timings in the scenarios are capture time, which runs faster than real time; the results
  do not depend on the machine's speed.
- **Frames can differ from run to run, even with the same build.** On the large town, and on
  the medium town at the highest video settings, a few frames come out in one of two versions:
  a few hundred pixels differ, mostly in the water's reflection of trees. The saves are always
  identical. The cause is not known yet. When a frame differs, run both builds again: a change
  is accepted when the new build's frames match one of the old build's versions.

## The save-timing trap

The first large-town comparison of the 0.2.0 performance work showed different saves: the
growth values of 835 trees differed. The cause was not the change. Headless captures wrote the
save after the game had drawn the save's preview image, and reading that image back from the
GPU took a varying number of frames. The game was paused at that point, but the growth smoothing
runs even when paused (as in the original, `FUN_1400c98a0`), so every extra frame moved the
values. Headless captures now wait for the GPU readback (`Renderer::blocking_readback`), and a
save lands on the frame it was asked for. Two runs of the same build agree, and so do the builds
before and after the performance work.
