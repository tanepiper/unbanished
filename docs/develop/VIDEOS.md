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

<video src="../assets/videos/citizens-market.webm" poster="../assets/videos/citizens-market-poster.webp" autoplay loop muted playsinline width="720"></video>

## Make the documentation's videos again

```bash
tools/videos.sh save-game/Save83.sav                    # the first four (chicken, market, zoom, seasons)
tools/videos.sh save-game/Save83.sav seasons            # one of them
tools/videos.sh LARGE.sav big-town                      # a large town (the 0.2.0 notes use 1,024 citizens)
tools/videos.sh save-game/Save83.sav orbit follow-citizen
tools/videos.sh RIVER.sav river fishing                 # a town with a bridge and a fishing dock
tools/videos.sh "save-game/Save 47.sav" boat-shoulder fisher-closeup river-flight   # the cinematic camera
tools/videos.sh save-game/Save83.sav farmer labourer vendor                        # people at work, close up
FILTER=sepia tools/videos.sh SAVE.sav orbit             # with a colour filter: sepia, grey or psychedelic
GAME=path/to/banished VIEWER=path/to/banished-viewer tools/videos.sh SAVE.sav   # without building
```

| Video | What it shows | How |
|---|---|---|
| `chicken-walk.webm` | A chicken's Walk animation, one loop | 74 viewer captures, posed with `VIEWER_POSE` |
| `zoom-into-town.webm` | The camera zooms from above a town down to its market | Frames 1.5 to 10 s after the zoom starts |
| `citizens-market.webm` | Citizens walk and trade at the market | 8 s at normal speed, the middle of the frame |
| `seasons.webm` | Autumn, rain, snow and the start of spring | A time-lapse at speed 10: one frame every 0.75 s for 180 s |
| `big-town.webm` | A large town at speed 10, the camera turning slowly over it | `tools/videos.sh LARGE.sav big-town`: the saved view, `--hold RotateLeft` for 8 s |
| `orbit.webm` | The camera circles the middle of a town | `--hold RotateRight` for 10 s, after `--hold PitchUp` lowers the camera |
| `follow-citizen.webm` | The camera follows one citizen at the closest zoom | `select citizen N`, then `--press buttonFollow` (`CITIZEN=N` chooses who) |
| `river.webm` | Along a river from a bridge | Centred on a bridge, then `--hold Back` for 7 s |
| `fishing.webm` | Fishers on their dock, at speed 3 | Centred on a fishing dock, close zoom, `--hold RotateLeft` for 4 s |
| `boat-shoulder.webm` | Over the trader's shoulder as the boat rows down the river, under a bridge and up to the trading post | `--camera-follow "boat 0@5"` with `--camera-rig 6,3.4,0,30,-1` |
| `fisher-closeup.webm` | A fisher with a rod at the end of the dock; the camera circles from behind them to their face | Three `--camera` keyframes round the fisher |
| `river-flight.webm` | A flight down the river into the town, past the bridges and the trading post, then up over the town | Six `--camera` keyframes in 10 s, along the boat's route |
| `farmer.webm` | A farmer harvesting a field by the market, then carrying the crop | `--camera-follow "profession farm 0@5"` at speed 2, in `Save83` |
| `labourer.webm` | A labourer walking past the log and stone stockpile | `--camera-follow "profession labor 1@5"` at speed 2 |
| `vendor.webm` | A vendor bringing goods to the market stalls | `--camera-follow "profession vendor 0@5"` at speed 2 |

Each video has a poster, `NAME-poster.webp`: one frame of the video, which a page shows until
the video plays (`<video poster=…>`).
| `montage.webm` | The home page's video, about 64 s, with an opening title, a caption for each shot and a closing card: a flight down a river into a town, the trader's boat, a fisher, the market, a farmer, a vendor and a labourer close up, an autumn town, a young town, a winter town, a year of seasons, and the camera rises away | Twelve shots from five saves, joined with 0.6 s transitions of eleven kinds (`xfade`: fade through black, slides, wipes, a circle, a radial sweep, a blur, fade through white) |

The montage is made only when you ask for it (`tools/videos.sh save-game/Save83.sav montage`),
because it loads five saves from the same folder and takes several minutes. Its shots are in the
script's `montage` part: most reuse the runs of the other videos (`run_flight`, `run_boat`,
`run_farmer`, …) and keep a part of their frames with `clip`; the others give the save, the
building the camera centres on, the zoom and any pointer moves. `joins` lists the transition
between each pair of shots, and `captions` the caption of each shot. A pointer moved to the edge
of the screen (`--move 0,360@T`) scrolls the camera, as in the game, and a `--move` back to the
middle stops it.

`MONTAGE_CLIPS=DIR` keeps the twelve shots in `DIR` as H.264 clips. When they are all there, the
next montage uses them and draws nothing, so changing a title or a transition takes seconds
instead of ten minutes:

```bash
MONTAGE_CLIPS=/tmp/montage-clips tools/videos.sh save-game/Save83.sav montage   # draws and keeps the shots
MONTAGE_CLIPS=/tmp/montage-clips tools/videos.sh save-game/Save83.sav montage   # only joins and titles them
```

### Titles

The montage's titles are drawn by FFmpeg's `drawtext` filter, after the game. It needs an FFmpeg
built with FreeType. Homebrew's default `ffmpeg` has no `drawtext`; `ffmpeg-full` does, and
installs beside it without replacing it:

```bash
brew install ffmpeg-full
```

`tools/videos.sh` uses, in this order: `FFMPEG` if you set it, Homebrew's `ffmpeg-full`, then the
`ffmpeg` on your `PATH`. Without `drawtext` it makes the montage without titles and says so.
`TITLES=0` leaves them out on purpose.

| Title | Where | Font |
|---|---|---|
| "Un-Banished" and a line under it | the middle of the opening flight, 0.6 to 5.5 s | Georgia Bold 64, Georgia Italic 24 |
| One caption for each shot ("The trader rows in", "Harvest", "Winter", …) | low on the left, on a dark band, from 0.8 s into the shot to its end | Georgia Italic 24 |
| "Un-Banished" and "Recorded by the game itself, without a window" | the middle of the outro, its last 4.4 s | Georgia Bold 56, Georgia Italic 22 |

Every title fades in and out over 0.6 s. The fonts are macOS system fonts; set `TITLE_FONT` and
`CAPTION_FONT` to other font files, or the script asks fontconfig for a serif font. Each text is
written to a file and read with `textfile`, so quotes and colons need no escaping. The `text`
function in the script makes one title: text, font, size, position, start and end times.

`cwebp` makes the poster images.

The script needs your game data in `game-data/`, a save of a town without mods, FFmpeg with
VP9 (libvpx), `bc` and `cwebp`. Every town video is drawn at 1920 × 1080 with the highest video
settings and scaled down, and has a poster, `NAME-poster.webp`. The town videos centre the camera
on a building in the middle of the town by clicking it. With a different save, set `CLICK=X,Y`
to a building there (pixels in a 1280 × 720 frame; the script scales them to the capture size).

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

### 2. Move the camera

There are four ways to move the camera in a headless run. They work as in the game, so the
camera eases in and out in the same way.

| Movement | Option | Example |
|---|---|---|
| Zoom | `--wheel STEPS@T`, one event for each step of the wheel | `--wheel 8@16 --wheel 8@16.2` |
| Turn, tilt, travel, zoom | `--hold ACTION@START-END` holds the key of a key-map action | `--hold RotateLeft@17-25` turns the camera for 8 seconds |
| Scroll | `--move X,Y@T` puts the pointer at the edge of the screen; a move back to the middle stops it | `--move 0,360@21 --move 640,360@22.5` scrolls left for 1.5 seconds |
| Follow a citizen | `select citizen N` (or `select profession NAME N`), then **Follow** | `--script "select citizen 5@14" --press buttonFollow@14.5` |

The actions for `--hold` are the names of the key map: `RotateLeft`, `RotateRight`, `Forward`,
`Back`, `Left`, `Right`, `ZoomIn`, `ZoomOut`, `PitchUp` and `PitchDown`. A short `PitchUp`
(about half a second) lowers the camera towards the horizon, which suits views along a river or
a street. Edge scrolling and the movement keys (`Forward`, `Back`, `Left`, `Right`) travel faster when the
camera is further away, so zoom in for a slow, steady movement.

### The cinematic camera

The game's own camera can only do what a player can do. For close-ups and flights, a capture can
draw the town from a camera of its own. It changes only the picture: the game camera, the
simulation and the save are the same as without it, and the shadows, the sky, the weather and the
choice of what to draw follow the new camera. The options are listed in the
[game's README](../../apps/game/README.md#the-cinematic-camera).

**A path of keyframes.** Each `--camera` gives where the camera is, what it looks at, and when:

```bash
banished --boot-capture /tmp/video/last.png 30.4 --capture-size 1920x1080 \
  --script "load save-game/Save 47.sav@4.5" \
  --camera "-75,74,24>-15,70,0@20" --camera "-30,80,20>25,52,0@22" \
  --camera "10,66,17>62,20,0@24" --camera "105,-20,45>70,-20,0@30" "${shots[@]}"
```

The camera moves on a smooth curve through the points. It starts and stops gently, and does not
slow down at the keyframes between. Positions are in map cells from the middle of the map, `z`
up; the ground is near 0 and water is at 0.

**Riding with a subject.** `--camera-follow` puts the camera behind and above a citizen, a trader
boat or an animal, looking where it goes:

```bash
  --camera-follow "boat 0@5" --camera-rig 6,3.4,0,30,-1 --camera-fov 50
  --camera-follow "profession hunt 0@20-35"      # the first hunter, from 20 to 35 seconds
  --camera-follow "animal deer 2@10"
```

`--camera-rig BACK,UP,SIDE,AHEAD,LOOK_UP` sets the place in the subject's own frame. The camera
turns smoothly with its subject (`--camera-smooth`), so a citizen's quick turns do not shake the
picture. It also keeps the buildings, trees and bridges near the subject out of the way, now
and a few steps ahead: if one would come between them, the camera first moves in closer to the
subject, and if that is not enough it rises (up to 8). It tests the line of sight against each
object's box, then against its picking mesh, the shape that mouse clicks hit, so it sees the
open square of a market but the roof of the market's storehouse. A citizen under a tree's crown
cannot be seen from anywhere near, so there the camera keeps its place.

To place keyframes, log a subject's positions: run with
`RUST_LOG=info,banished::view::cinematic=debug` and `--camera-follow`. A trader boat's positions
trace the river; a citizen's lead to their workplace.

Unlike the game camera, the cinematic camera does not depend on which frames are drawn: it moves
with the capture's time. A video can start at any frame.

### Quality and filters

The cinematic videos use the best picture the game can draw, and are drawn larger than the video
and scaled down. In `tools/videos.sh` this is the `HQ` list:

```bash
--capture-size 1920x1080 --setting VideoMultisample=3 --setting VideoTextureFilter=16 \
  --setting VideoShadowScalar=0 --setting VideoShaderDetail=3 --setting VideoReflectionDetail=2
```

That is 4× antialiasing, 16× anisotropic filtering, 2048 × 2048 shadow maps, high shadow quality
and reflections of everything. A 1920 × 1080 frame takes about three times as long as a
1280 × 720 one, and each BMP is 6 MB, so 300 frames need about 2 GB of scratch space.

`FILTER` changes the colours of every video that `tools/videos.sh` makes, in FFmpeg after the
game: `sepia`, `grey` or `psychedelic` (the hues turn through the colour wheel). The file name
gets `-FILTER` at the end, for example `orbit-sepia.webm`.

### 3. Queue one screenshot for each frame

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

### 4. Join the frames

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

### 5. Check it

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
