# Game loop step 01: frame update and simulation gating

This is the first focused pass through the original `Game/Game` loop. It records the outer frame
order and pause/speed gates before tracing the individual simulation managers. Evidence is static
Ghidra 12.1.4 pseudocode for the Steam x64 Windows executable, not a live runtime capture or source
code. The executable SHA-256 is recorded in [GAMEPLAY.md](GAMEPLAY.md).

## Frame order observed

`FUN_14009b850` is the `Game/Game` update method identified by the loop's vtable export. It first
asks `UserInterface::PausesGame` about the current UI. If the UI does not pause play, the method
reads the simulation controller at `Game/Game +0x120`:

1. When the controller byte at `+0x30` is clear, its integer at `+0x34` supplies the number of
   simulation iterations for this rendered frame. Otherwise, the count stays zero.
2. For each iteration, the loop advances a manager reached through `Game/Game +0xf8` (`+0x78` on
   that object), updates the animation manager at `+0x68`, calls `FUN_1400c9560` on the manager at
   `+0xe8`, and advances frame/tick counters at `+0x118`.
3. Animation update receives `true` on the final iteration and `false` on earlier iterations.
4. The loop then updates achievement state, applies the camera transform, updates the UI, and calls
   `FUN_140107b70` with the frame delta and the inverse of the UI flag at `+0xd9`.

The helper `FUN_140107b70` is not a calendar clock. Its call to `FUN_140104390` updates the camera
from keyboard and edge-scroll input. `FUN_140104bb0` handles the pause toggle and camera-preset
shortcuts. The object at `+0x28` provides camera-preset support and participates in controller
switching. The simulation manager at `+0xe8` is the separate entity scheduler described in
[GAME-LOOP-STEP-02.md](GAME-LOOP-STEP-02.md) and [ENTITY-COMPONENT-LIFECYCLE.md](ENTITY-COMPONENT-LIFECYCLE.md).

## Port boundary

The current port's calendar advances from wall-clock time at an invented 12-second month interval.
The Ghidra trace establishes a UI pause check and a separate simulation iteration count, but does
not establish the original interval or calendar transition rules. Those remain open. The original
toolbar's iteration values are now traced as 1, 2, 5 and 10 in
[GAMEPLAY-SPEED-AND-TOOLBAR.md](GAMEPLAY-SPEED-AND-TOOLBAR.md). The evidence also establishes that
a paused UI bypasses the outer per-iteration manager block; the active UI update path still runs
afterward.

## Evidence pointers

- `analysis/export/Application-steam-x64.exe/decomp.c`, `FUN_14009b850` at `0x14009b850`.
- Same export, `FUN_140107b70` at `0x140107b70`.
- `analysis/gameplay-gameloop-methods.txt`, exported excerpts of both functions.
- Ghidra binary identity and repeatable export workflow: [GAMEPLAY.md](GAMEPLAY.md).

## Next trace

The toolbar's selected value and UI dispatch are traced in
[GAMEPLAY-SPEED-AND-TOOLBAR.md](GAMEPLAY-SPEED-AND-TOOLBAR.md). Continue with the calendar and its
displayed month/year transitions; do not equate the entity scheduler's accumulated elapsed time
with the calendar.
