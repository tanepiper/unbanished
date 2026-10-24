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
   `cargo run -q --release -p unbanished-app --example imgdiff -- game-data/reference/ui/main-menu.png port.png --resize --blur 2 --out heat.png`.
   To reproduce New Game settings chosen in the original, script the port's dialog:
   `UNBANISHED_SEED=SEED unbanished --boot-capture OUT T --capture-size WxH --press buttonNew@4.5
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
UNBANISHED_SEED=999518904 tools/reference/compare.sh seed-999518904/start-t0 --ignore 0,0,120,130 -- \
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

## The original in an emulated PC

The original also runs on this Mac in the project's own emulated PC: QEMU emulating an x86-64
PC, Debian, Wine, Xvfb and software OpenGL, written by GameFrame's `tools/live/emupc.py`. It reaches
the main menu ("Version 1.0.7 - Build 170910") with Direct3D 9 in a window. It is slow, but an agent
can start it, drive it with `xdotool`, make saves and take lossless screenshots (`import -window root`)
without a Windows PC. Everything lives in `derived/vm` (git-ignored): the disk, its SSH key and the
VNC password. Measured on 2026-10-24.

Setting it up:

1. `UNBANISHED_VM_IMAGE=PATH/debian-13-genericcloud-amd64.qcow2 tools/reference/vm/setup.sh`
   writes the VM (port 2242, name `gf-vm-banished-wasm`, 8 GB, 6 CPUs), boots it and installs Wine,
   64-bit Mesa, the WMA decoder and `x11vnc`. `tools/gf.sh tools/live/emupc.py image` says where the
   image comes from.
2. Steam. The Runtime quits at start when `SteamAPI_Init` fails (`Engine::Engine::PlatformStartup`,
   `1800a3020`), so the VM needs the Windows Steam client: install `SteamSetup.exe /S` into the
   prefix `~/.wine-banished`, start it, and have the owner sign in through VNC (set a password with
   `x11vnc -storepasswd`; macOS Screen Sharing refuses a server without one; tunnel it with
   `ssh -F derived/vm/ssh_config -f -N -L 127.0.0.1:5942:127.0.0.1:5900 gf-vm-banished-wasm`).
   Agents never type the owner's credentials. Steam remembers the sign-in.
3. Banished in Steam's library: copy `game-data/original/` to
   `steamapps/common/Banished` and write `steamapps/appmanifest_242920.acf` with `StateFlags` 4.
   Started from any other folder, Steam stops it with "Application load error 6:0000065432".
4. Once, in the game's launcher, choose Video Options: DirectX 9.0c, Fullscreen off, VSync off.

Running it: `ssh -F derived/vm/ssh_config gf-vm-banished-wasm bash run-original.sh` starts Steam,
the display and VNC when they are not running, then the game, and saves the screen to
`/tmp/banished.png` in the VM. Press Play in the launcher; the main menu appears after about three
minutes. `bash run-original.sh stop` stops only the game.

What each step fixed (all *measured*):

| Symptom | Cause | Fix |
|---|---|---|
| Exits at once, code 53, no window | Without Steam, `SteamAPI_Init` fails | Steam signed in |
| Exits with 53 from Steam's library | `steam_api64` hands the start to `steam.exe` | `SteamAppId=242920` in the environment |
| "Application load error 6:0000065432" | Started outside Steam's library | Copy into `steamapps/common` with a manifest |
| Stops after the splash, no CPU use | Steam's overlay crashes in `gameoverlayrenderer64.dll` | Rename `GameOverlayRenderer*.dll` (the script does) |
| Access violation in `xaudio2_7.dll` | Wine has no WMA decoder for the game's xWMA sounds | `gstreamer1.0-libav`, and a silent ALSA `null` device |
| Guest kernel panic (`int3` in `finish_task_switch`) | QEMU's software CPUs while the kernel patches itself | Restart the VM; use fewer CPUs if it comes back |

Saves the original makes go to its own save folder in the prefix (`Documents\Banished`); copy them
out with `scp` into `save-game/`, which git ignores.
