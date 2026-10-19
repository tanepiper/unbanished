---
name: banished-video
description: Record short videos of the running Banished reimplementation without a window — frame sequences from headless boot captures (`--script "screenshot@T"`) and posed viewer captures (VIEWER_POSE), joined with FFmpeg into WebM. Use to show or check motion (animations, citizens walking, camera moves, weather, seasons), to make documentation videos (tools/videos.sh), or to compare timing with the original game.
---

# Record a video headlessly

A video is a sequence of frames. The game can already save frames: the `screenshot` script
command writes `Screenshots/Screenshot<N>.bmp` in the user folder, with N counting up from 0.
So queue one `screenshot` command per video frame in a `--boot-capture` run, then join the BMPs
with FFmpeg. No Rust change is needed. The person-readable guide is `docs/develop/VIDEOS.md`;
`tools/videos.sh` makes the documentation's videos and is the worked example.

## Rules

- Always set `BANISHED_USER_DIR` and `BANISHED_SETTINGS` to a scratch folder (the frames and any
  save go there; the player's own folders are never touched).
- Frames go to scratch or `game-data/derived/` while you work. Only the finished, small WebM of
  Un-Banished's own frames may be committed, in `docs/assets/videos/` (AGENTS.md rule 1); never
  commit the BMP/PNG frames or footage of the original game.
- Look at the result before you report it: extract a few frames into one contact sheet and
  open it with Read (see "Check" below).

## Steps

1. **Find the shot.** Make single `--boot-capture` PNGs first (cheap: ~5 s for a 20 s run) and
   open them. Typical set-up for a town:
   ```bash
   export BANISHED_USER_DIR=$W/user BANISHED_SETTINGS=$W/settings.txt
   banished --boot-capture $W/shot.png 30 --capture-size 1280x720 \
     --script "load save-game/Save83.sav@4.5" \
     --click-at 1060,470@14 --press buttonCenter@14.6 --press buttonClose@15.2 \
     --wheel 8@16 --wheel 8@16.2 ...            # 28 wheel events reach the closest zoom
   ```
   - The wheel zooms towards the **screen centre**, not the pointer. To move the camera, click a
     building (`--click-at X,Y@T`), press its window's `buttonCenter`, then `buttonClose`.
     `buttonFollow` in a citizen's window follows that citizen.
   - `--script "speed 10@T"` for time-lapses; `--script "open Template/UtilityMap.rsc@T"` for
     windows; `BANISHED_SEED=N` with `--press buttonNew@4.5 --press buttonOk@5.0` for a new
     game.
2. **Queue the frames.** One `--script "screenshot@T"` per frame (`screenshot ui@T` keeps the
   interface). Two rules make the timing exact:
   - put each time **half a frame after** the frame time, `T = start + i/30 + 1/60`: the capture
     loop steps 1/30 s in f32, and a command exactly on a frame boundary can land on the same
     frame as its neighbour (one run gave 137 files for 180 commands);
   - **draw every frame from before the camera moves** (start the screenshots at the first
     click or wheel, and drop the early frames when encoding). A headless run draws only when
     something asks for a frame, and the camera's easing depends on the drawn frames: a camera
     that was not drawn for seconds jumps, then drifts.
   For a time-lapse, use a longer step instead of 1/30 (seasons: 0.75 s at speed 10).
3. **Encode** (VP9 needs yuv420p; the BMPs are RGB):
   ```bash
   ffmpeg -framerate 30 -start_number 360 -i $W/user/Screenshots/Screenshot%d.bmp -frames:v 240 \
     -vf "crop=960:540:160:90,format=yuv420p" -c:v libvpx-vp9 -b:v 0 -crf 36 -row-mt 1 -an out.webm
   ```
   Keep documentation videos small: 384–960 px wide, 2–12 s, CRF 34–42, under ~1 MB each.
   Cropping the middle of a 1280x720 capture makes citizens larger than zooming can.
4. **Animations of one model** come from the viewer, not the game: one `--capture` run per frame
   with `VIEWER_POSE=CLIP@SECONDS VIEWER_ANIMS=GROUP` (filters match the backslash entry name,
   e.g. `'Animals\Chicken\ChickenMesh'`). ~0.35 s per frame.

## Check

```bash
ffmpeg -i out.webm -vf "select='not(mod(n\,40))',scale=320:180,tile=6x1" -frames:v 1 sheet.png
```
Open `sheet.png`: the frames must change (motion, weather) and the camera must not jump. Count
the BMPs: it must equal the number of `screenshot` commands. A `--press NAME: not found at T s`
in the log is a failure.

Runs are deterministic: the same binary, save and arguments give the same frames
(`ffmpeg -i a.webm -f md5 -` on two runs). So a video can also compare two builds frame by
frame, like `tools/triage/determinism.sh` does for one frame.

## Costs (MacBook Pro M3, release build)

| Run | Time |
|---|---|
| 20 s capture, a few frames | ~5 s |
| 600 frames drawn at 1280x720 (34 s capture) | ~20 s |
| 200 s at speed 10, 240 frames | ~90 s |
| 74 posed viewer frames | ~25 s |
