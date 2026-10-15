# Reference frames from the original game

The original Banished can run on a Windows PC and stream to this Mac through Steam Remote
Play. Agents with computer-use access (the Steam app granted) can watch and drive it, which gives
ground truth no static analysis can: what the original actually shows and does.

Captures are **local only** (`game-data/reference/`, git-ignored); never commit them.

## Workflow

1. Find the stream window: `swift tools/reference/windows.swift steam` (or the computer-use
   `app_list_windows` for Steam). Lossless `screencapture -l ID` needs Screen Recording
   permission for the agent's process; without it, use the computer-use `app_screenshot` of the
   stream window and file its saved image.
2. File it: `tools/reference/save.sh <screenshot> ui/main-menu "1280x800 window, default UI"`.
3. Capture the same view in the port (`--boot-capture` with scripted presses, see the
   `banished-verify` skill) and compare:
   `cargo run -q --release -p banished-app --example imgdiff -- game-data/reference/ui/main-menu.png port.png --resize --blur 2 --out heat.png`.
   The stream is video-compressed: compare with `--blur 2` and a threshold around 24, and judge
   layout from the grid and bounding box rather than PSNR alone.

## What to capture (highest value first)

| Reference | Why it is a 1:1 check |
|---|---|
| UI dialogs (main menu, New Game, toolbar, status bar, citizen/building windows) | Static layout and text: positions, fonts, colours compare pixel-for-pixel |
| New Game with a fixed seed: minimap and the start view | Terrain generation, start point, seeding and start structures are deterministic for a seed |
| First minutes of a seeded game | Behaviour: which activities citizens choose first (tools, clothing, work), walking paths |
| Toolbar placement ghosts and CreateBar text | Placement rules, validity colours and costs |

Record what you observe (not only images) in the topic doc under an **Observed in the original**
heading, with the seed, settings and in-game time, so the fact can be re-checked.
