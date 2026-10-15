# Observed in the original

Facts read off the running original game (Steam 1.0.7, streamed from a Windows PC through Steam
Remote Play), compared with the port at the same resolution with `imgdiff`. The reference frames
themselves stay local (`game-data/reference/`, see `tools/reference/README.md`); each entry
names its reference id, the settings and how the port was captured, so it can be re-checked.

Capture conditions so far: the streaming client window is 1728×1084 points on a Retina display,
so the original renders at **3456×2168** and the stream is downscaled; computer-use screenshots
of the window are 1384×868 JPEG. Banished draws its UI at native pixel size, so the port must be
captured at 3456×2168 (`--boot-capture … --capture-size 3456x2168`) and resized for comparison;
at 1384×868 the port's UI is about 2.5× too large relative to the scene.

## Main menu (`ui/main-menu`)

Port: `BANISHED_MENU_SCENE=6 banished --boot-capture OUT 4.5 --capture-size 3456x2168`;
compare `imgdiff ui/main-menu.png OUT --resize --blur 3 --threshold 32 --compose panels.png`.

- **Scene:** the original showed `Game/MenuScene/Scene6` (fields, orchard, three houses, well,
  pasture). Scoring all ten port scenes against the frame picks 6 (lowest MAE after scene 5);
  confirmed by eye.
- **Matches:** menu scene content and placement, menu button column position and size (within a
  few pixels at 1384 width), menu scene camera within the sway range.
- **Shadows (ported, see [SHADOWS.md](SHADOWS.md)):** the original casts shadows from trees,
  buildings, fences and rocks onto the terrain. The port now draws the sun's shadow map; its
  orchard and building shadows fall in the same direction with the same softness. Captured at
  3456×2168, MAE 22.3 → 19.8 and PSNR 16.9 → 17.8 dB. The ground stays as bright as the
  original's: pasture red mean 0.42 in the port vs 0.40 in the original. The original's
  depth pass culls opposite faces, so the terrain does not shadow itself.
- **Frame and logo (open):** the original's torn-edge band spans about y 150–735 of 868; the
  port's spans about y 120–765. The logo sits lower in the original (top about y 75 vs 35) and
  the copyright/build line higher (about y 810 vs 855). The backdrop/logo layout of
  `Dialog/MainMenu.rsc` at this aspect ratio needs checking against the dialog's anchoring.
- **Colour (open):** the original's scene is darker and less saturated (warmer greens). With
  shadows ported, the rest is still open: possibly fog/ambient values or a post-process.
- **Camera timing:** the camera sways (`angle += dt·11.25`, yaw `base + 2·cos(angle)`), so the
  port's capture time must match the original's time in the menu for a pixel-level camera
  comparison; layout differences above are larger than the sway.

## New Game dialog (`ui/new-game`)

Captured with the original at a lower PC resolution (UI larger in the 1384×868 frame). Defaults
shown when the dialog opened: town name **Sevelockwell**, map seed **999518904**, terrain
Mountains, size Small, climate Fair, disasters On, starting conditions Medium (the user had
changed terrain/size before the capture). The description text under the combos is the Medium
start condition's text.

**Checked — world generation for this seed (match):** the port's
`banished --capture OUT --seed 999518904 --terrain Mountains --size 0 --view overview` shows a
river entering at the north edge, a lake just south-west of the centre with the river leaving
south, a second small lake to its east, and mountains on all sides. The original's minimap
after starting with these settings shows the same layout (`seed-999518904/start-t0`).

## Seeded start (`seed-999518904/start-t0`, `start-t1`)

Original: Sevelockwell, seed 999518904, Mountains, Small, Fair, disasters On, Medium; captured
at the start view (t0, minimap open) and about 20 s later (t1). Port:
`BANISHED_SEED=999518904 banished --boot-capture OUT 12 --capture-size 1384x868 --press
buttonNew@4.5 --combo comboTerrain=Mountains@4.8 --combo comboTerrainSize=Small@4.8 --press
buttonOk@5.0`.

- **Matches:** minimap (river from the north into the central lake, smaller eastern lake);
  start point (barn between the same hills, river to the east); rock clusters and tree groups
  around the start; the yellow "homeless" status icon over every citizen.
- **Stockpile (open):** the original has a stockpile of goods (crates, logs, stone) beside the
  barn; the port draws only the barn. The Medium profile's yard/stockpile structure and its
  goods models are missing in the port.
- **Citizens at start (open):** at t0 the original's citizens stand in a ring of about 5–8
  cells around the barn (household spread); at t1 a group gathers at the stockpile's north
  edge and the barn door while others loiter, consistent with fetching tools and clothing
  first (activities 8 and 3). The port's citizens are clumped at the barn door 7 s in; compare
  the household spread (`MapGenerator +0x40`) and spawn positions at t = 0.
- **Camera (fixed):** the port's camera now follows the original (`FUN_14009bfd0`
  → `FUN_14009da80`: yaw 0, pitch −40, distance 25; controller `FUN_140104390` each frame:
  zoom target ±5 per wheel step within 20–130, pitch target from zoom and the 0.75 pitch
  slider, 10 % easing of both, pivot clamped to `± n/2 · clamp(1 − (d − 100)/150)`;
  projection `FUN_14009db30(40, 2, 512)`). The reference frame was taken zoomed fully out;
  the port zoomed out the same way (25 wheel steps) reproduces its framing: barn, hills,
  citizens and lake overlap in a 50/50 blend, and the river is within a few pixels
  (16:9 crop, MAE 16.7 vs 37 before).
- **Forests and palette (open):** with the framing matched, the original's forests are darker
  and denser (notably east of the river), and its colours are hazier; compare tree models,
  LOD/billboard shading and fog.
- **Trees (check):** the original's forests look denser and rounder from above; compare
  species/variant choice and density for this seed.
- **Shadows (ported):** tree, building and rock shadows now darken the forests as in the
  original (compare.sh: MAE 17.1 → 15.7, PSNR 21.1 → 21.7 dB). Not yet matched: casters off
  screen still cast, because the original culls them with the main camera's frustum.

## Capture pitfalls

- The PC's resolution sets the frame: at 16:9 inside the 16:10 stream window the reference has
  black bars (45 px top and bottom at 1384×868). Crop it (`--crop-a 0,45,1384,778`) and capture
  the port at 16:9 (`--capture-size 1384x778`).
- Mask the original's open minimap and toolbar (`--ignore`) when they differ from the port.
- Scripted wheel lists in zsh need `${=W}` (unquoted `$W` stays one argument).

## Driving the original

Mouse movement and hover reach the game through the Steam streaming client, but synthetic
clicks and key presses from computer use are dropped (tried background clicks, held presses and
foreground clicks with full-screen control). Real clicks from the user work. Until that changes,
the user sets up a view and agents capture it in the background with `app_screenshot`.
