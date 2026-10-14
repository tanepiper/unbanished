# Camera input regression: gameplay UI capture

## Finding

The gameplay loop added a full-screen UI root. The shared input adapter treated the existence of
any UI root as ownership of all drag, wheel and keyboard input. As a result, map drags and zooms
never reached the gameplay camera after the HUD appeared. The same routing also kept Escape and
WASD/arrows from reaching the gameplay loop whenever the HUD was active.

## Fix

- UI reports whether the current hit target is an interactive control and whether a text edit has
  keyboard focus.
- Left press over an interactive control is captured by the UI; left drag over empty map space and
  right drag rotate the camera. Middle drag pans the map.
- Wheel events first offer scrolling to the UI, then zoom the camera when no scrollable control
  consumes them.
- Escape and camera movement keys pass through when UI key handling does not consume them, while
  text editing keeps its key input.
- The active drag records its mouse button, so releasing another button does not cancel it. Losing
  window focus clears the drag state in case the release event was missed.

## Evidence and limit

The regression was established by source tracing in `apps/game/src/main.rs` and
`crates/ui/src/input.rs`: gameplay always returns an input root, while the old `Game::drag` and
`Game::wheel` methods only forwarded to the camera when there was no root. The input routing now
preserves map gestures alongside the HUD. The explicit drag state and middle-button pan address
missed releases and provide a direct pan gesture. Interactive runtime verification is not recorded
here.
