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
   To reproduce New Game settings chosen in the original, script the port's dialog:
   `BANISHED_SEED=SEED banished --boot-capture OUT T --capture-size WxH --press buttonNew@4.5
   --combo comboTerrain=Mountains@4.8 --combo comboTerrainSize=Small@4.8 --press buttonOk@5.0`
   (`--combo NAME=ITEM@SECONDS` selects by the item's text).
   The stream is video-compressed: compare with `--blur 2` and a threshold around 24, and judge
   layout from the grid and bounding box rather than PSNR alone.

## One-step comparison

`tools/reference/compare.sh REF [--no-autocrop] [--ignore x,y,w,h]... -- SECONDS PORT-ARGS...`
crops the reference's black bars (`imgdiff --autocrop-a`; disable for the main menu), captures
the port at the remaining size with the scripted options, and writes `port.png`, `panels.png`
(reference | port | blend), `heat.png` and `report.json` under
`game-data/reference/compare/REF/`. The seeded start reproduces with:

```bash
W=$(python3 -c "print(' '.join(f'--wheel -1@{9+i*0.05:.2f}' for i in range(25)))")
BANISHED_SEED=999518904 tools/reference/compare.sh seed-999518904/start-t0 --ignore 0,0,120,130 -- \
  16 --press buttonNew@4.5 --combo comboTerrain=Mountains@4.8 --combo comboTerrainSize=Small@4.8 \
  --press buttonOk@5.0 ${=W}   # zsh; use $W in bash
```

## What to capture (highest value first)

| Reference | Why it is a 1:1 check |
|---|---|
| UI dialogs (main menu, New Game, toolbar, status bar, citizen/building windows) | Static layout and text: positions, fonts, colours compare pixel-for-pixel |
| New Game with a fixed seed: minimap and the start view | Terrain generation, start point, seeding and start structures are deterministic for a seed |
| First minutes of a seeded game | Behaviour: which activities citizens choose first (tools, clothing, work), walking paths |
| Toolbar placement ghosts and CreateBar text | Placement rules, validity colours and costs |

Record what you observe (not only images) in the topic doc under an **Observed in the original**
heading, with the seed, settings and in-game time, so the fact can be re-checked.
