# Camera input regression: gameplay UI capture

## Finding

The gameplay loop added a full-screen UI root. The shared input adapter treated the existence of
any UI root as ownership of all drag, wheel and keyboard input. As a result, map drags and zooms
never reached the gameplay camera after the HUD appeared. The same routing also kept Escape and
WASD/arrows from reaching the gameplay loop whenever the HUD was active.

## Fix

- UI reports whether the current hit target is an interactive control and whether a text edit has
  keyboard focus.
- Left press over an interactive control is captured by the UI. Superseded (2026-10-16): the
  original turns the camera only while the `MouseMiddle` button (the middle one) is held, the
  right button cancels the tool and nothing pans by mouse; see KEYMAP-AND-CAMERA-CONTROLS.md.
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

## Follow-up: new-game loop boundary

The new gameplay path still forwards camera input through the same chain: the window handler sends
drag and wheel gestures to `Game`, `MainLoop::raw` forwards them to the current loop, and
`GameplayLoop::raw` updates the `SceneCamera::Map` camera. The full-screen gameplay HUD only captures
left presses whose current hit target is an interactive control; wheel input falls through unless
a scrollable UI control consumes it, and right/middle drags bypass UI capture.

`MainLoop::raw` ignores input while a fade transition has a pending loop. Loading/dialog loops also
have no map camera. Once `GameplayLoop` is current, the source path is present. I have not isolated a
source-level loss in that settled path. The user reports that the camera no longer responds; the
interactive reproducer (platform and gesture) remains unconfirmed, so this is recorded as an open
runtime regression rather than calling the earlier source-traced fix verified.

The current environment had the browser dev server running, but access to its Arc window was denied
by the computer-use approval gate. No interaction claim is made from that unavailable browser
session.

## Source audit for the reported loss of control

A fresh static audit of `apps/game/src/main.rs`, `apps/game/src/gameplay.rs`, and
`apps/game/src/loops.rs` did not find a persistent input-routing break. Cursor movement forwards
left/right drag deltas to `Game::drag`, middle drag to `PanDrag`, and the wheel to `Game::wheel`;
`GameplayLoop::raw` applies rotate, pan, and zoom to the map camera. Left-button capture is limited
to an interactive UI hit target. The latest source change refreshes that hit target at press time,
which closes the stale-menu-hover path documented above.

The one confirmed suppression is `MainLoop::raw`: it ignores raw input while a loop transition is
pending (`apps/game/src/loops.rs`). This can drop gestures during a fade, but cannot explain camera
controls staying unresponsive after the gameplay loop settles. There is still no platform and
gesture reproduction, so a persistent runtime regression remains unresolved.

## Follow-up: stale hit target after a loop switch

The game keeps the UI's last hovered control while a loop changes its screen. If the menu is
replaced by gameplay under a stationary cursor, there may be no new cursor-move event before the
next mouse press. In that case the old menu button can still be marked as the hit target. The
first left drag is incorrectly captured by the UI instead of rotating the camera. The native
event
handler now refreshes the pointer hit target against the current loop immediately before deciding
whether a left press starts a camera drag. This is a source-traced regression fix; it has not yet
been confirmed in an interactive runtime.
