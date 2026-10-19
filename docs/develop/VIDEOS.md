# Recording videos

Un-Banished can record short videos of itself without a window. Nobody needs to sit at the
screen, and an AI agent can do it in the same way as a person. The videos in this documentation
were made like this, by `tools/videos.sh`.

The method needs no special video code. It uses two features that already exist:

- the game's `screenshot` command, which saves the current frame as a file;
- headless captures (`--boot-capture`), which run the game for a fixed time and accept
  commands at given times.

So a video is a headless run with one `screenshot` command for each frame. FFmpeg then joins the
frames into a WebM file.

<video src="../assets/videos/citizens-market.webm" autoplay loop muted playsinline width="720"></video>

## Make the documentation's videos again

```bash
tools/videos.sh save-game/Save83.sav                    # all of them
tools/videos.sh save-game/Save83.sav seasons            # one of them
GAME=path/to/banished VIEWER=path/to/banished-viewer tools/videos.sh SAVE.sav   # without building
```

| Video | What it shows | How |
|---|---|---|
| `chicken-walk.webm` | A chicken's Walk animation, one loop | 74 viewer captures, posed with `VIEWER_POSE` |
| `zoom-into-town.webm` | The camera zooms from above a town down to its market | Frames 1.5 to 10 s after the zoom starts |
| `citizens-market.webm` | Citizens walk and trade at the market | 8 s at normal speed, the middle of the frame |
| `seasons.webm` | Autumn, rain, snow and the start of spring | A time-lapse at speed 10: one frame every 0.75 s for 180 s |

The script needs your game data in `game-data/`, a save of a town without mods, FFmpeg with
VP9 (libvpx) and `bc`. The town videos centre the camera on a building in the middle of the town
by clicking it. With a different save, set `CLICK=X,Y` to a building there (pixels in a
1280 × 720 frame).

The runs are deterministic. The same program, save and arguments give the same frames, so a
video made again is identical frame by frame. This also means that two builds of the game can be
compared by their videos.

## Make your own video

### 1. Find the shot

Make single pictures first. A 20-second run takes about 5 seconds.

```bash
export BANISHED_USER_DIR=/tmp/video/user BANISHED_SETTINGS=/tmp/video/settings.txt
banished --boot-capture /tmp/video/shot.png 30 --capture-size 1280x720 \
  --script "load save-game/Save83.sav@4.5" \
  --click-at 1060,470@14 --press buttonCenter@14.6 --press buttonClose@15.2 \
  --wheel 8@16 --wheel 8@16.2 --wheel 8@16.4
```

- Always set `BANISHED_USER_DIR` and `BANISHED_SETTINGS` to a scratch folder. The frames are
  written there, and your own saves and settings stay untouched.
- The mouse wheel zooms towards the middle of the screen, not towards the pointer. To move the
  camera, click a building, press **Center** in its window (`buttonCenter`), then close the
  window (`buttonClose`). In a citizen's window, **Follow** (`buttonFollow`) makes the camera
  follow the citizen.
- 28 wheel events of 8 steps reach the closest zoom.
- `--script "speed 10@T"` speeds up the game, for time-lapses.

The [game's README](../../apps/game/README.md#headless-captures) lists every option and
command.

### 2. Queue one screenshot for each frame

At 30 frames per second, frame `i` is at `start + i / 30` seconds. Put each command half a frame
later, at `start + i / 30 + 1 / 60`. The capture adds 1/30 s for each frame, and a command
exactly on a frame boundary can land on the same frame as the next one. Then that frame is saved
only once, and the video jumps.

```bash
shots=()
for i in $(seq 0 239); do
  shots+=(--script "screenshot@$(echo "scale=5; 14 + $i / 30 + 1 / 60" | bc)")
done
banished --boot-capture /tmp/video/last.png 22.1 --capture-size 1280x720 \
  --script "load save-game/Save83.sav@4.5" "${shots[@]}"
ls /tmp/video/user/Screenshots | wc -l       # must be 240
```

`screenshot` saves the frame without the interface; `screenshot ui` keeps it.

**Draw every frame from before the camera moves.** A headless run draws a frame only when
something asks for one, and the camera's smooth movement depends on the frames that are drawn.
If nothing is drawn for some seconds, the camera jumps when drawing starts again, then moves
slowly to its place. So start the screenshots at the first click or wheel event, and drop the
early frames when you encode.

### 3. Join the frames

```bash
ffmpeg -framerate 30 -start_number 0 -i /tmp/video/user/Screenshots/Screenshot%d.bmp \
  -vf "scale=960:540,format=yuv420p" -c:v libvpx-vp9 -b:v 0 -crf 36 -row-mt 1 -an video.webm
```

- `format=yuv420p` is needed, because the screenshots are RGB and VP9 expects YUV.
- `-start_number N -frames:v COUNT` chooses a part of the frames.
- `crop=960:540:160:90` takes the middle of a 1280 × 720 frame. Citizens then look larger than
  the closest zoom can make them.
- For the documentation, keep each video short (2 to 12 seconds) and small (under about 1 MB):
  384 to 960 pixels wide, with `-crf` from 34 to 42.

### 4. Check it

Make a sheet of a few frames and look at it:

```bash
ffmpeg -i video.webm -vf "select='not(mod(n\,40))',scale=320:180,tile=6x1" -frames:v 1 sheet.png
```

The frames must change where you expect motion, and the camera must not jump.

## Animations of one model

The viewer poses a model with an animation clip at a given time:

```bash
for i in $(seq 0 73); do
  VIEWER_POSE="Walk@$(echo "scale=4; $i / 30" | bc)" VIEWER_ANIMS=Chicken \
    banished-viewer --capture /tmp/chicken 'Animals\Chicken\ChickenMesh'
  mv /tmp/chicken/Models_Animals_Chicken_ChickenMesh_resource.png /tmp/chicken/$(printf %03d $i).png
done
ffmpeg -framerate 30 -i /tmp/chicken/%03d.png -vf "format=yuv420p" -c:v libvpx-vp9 -b:v 0 -crf 34 chicken.webm
```

The filter matches the entry name in the package, which uses backslashes. Each frame takes about
0.35 seconds.

## What may be committed

Only the finished, short WebM files of Un-Banished's own frames, in `docs/assets/videos/`. Never
commit the frames themselves, or footage of the original game (see rule 1 in
[AGENTS.md](../../AGENTS.md)).

## For AI agents

The skill `.claude/skills/banished-video/SKILL.md` gives the same method in a short form, with
the costs of each kind of run.
