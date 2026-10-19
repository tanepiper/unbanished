#!/usr/bin/env bash
# Make the short WebM videos of the documentation in docs/assets/videos/.
#
#   tools/videos.sh SAVE.sav [NAME...]      # NAME: chicken-walk citizens-market zoom-into-town seasons
#
# SAVE.sav is a town saved without mods; the town videos load it. They were first made from a
# 116-citizen town in late summer. With another save, set CLICK=X,Y to a building in the town
# centre (pixels in a 1280x720 frame): the camera centres on it before it zooms in.
#
# How it works (see docs/develop/VIDEOS.md): the game runs headless (--boot-capture) and a
# `screenshot` script command is queued for each video frame. Every command writes the next
# Screenshots/Screenshot<N>.bmp in the scratch user folder, and FFmpeg joins them. The chicken is
# posed by the viewer (VIEWER_POSE), one frame per run. Needs game-data/ (tools/extract-assets.sh),
# ffmpeg with libvpx, and bc. The player's own saves and settings are never touched.
set -euo pipefail

repo="$(cd "$(dirname "$0")/.." && pwd)"
save="$(cd "$(dirname "${1:?usage: tools/videos.sh SAVE.sav [NAME...]}")" && pwd)/$(basename "$1")"
shift
names=("$@")
[[ ${#names[@]} -gt 0 ]] || names=(chicken-walk citizens-market zoom-into-town seasons)
out="$repo/docs/assets/videos"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
mkdir -p "$out"

# GAME and VIEWER take binaries that are already built (for example from tools/compat/build.sh).
if [[ -z "${GAME:-}" || -z "${VIEWER:-}" ]]; then
  cargo build -q --release -p banished-app -p banished-viewer --manifest-path "$repo/Cargo.toml"
fi
game="${GAME:-$repo/target/release/banished}"
viewer="${VIEWER:-$repo/target/release/banished-viewer}"
click="${CLICK:-1060,470}"

# frames FIRST STEP COUNT: set `shots` to `screenshot` commands at FIRST, FIRST+STEP, …
# (seconds). Each time is moved half a frame (1/60 s) later, so that no two commands fall on the
# same 1/30 s frame.
frames() {
  local i
  shots=()
  for ((i = 0; i < $3; i++)); do
    shots+=(--script "screenshot@$(echo "scale=5; $1 + $i * $2 + 1/60" | bc)")
  done
}

# town SECONDS OUT [ARGS...]: load the save, centre the camera on the building at $click, zoom
# in, and run until SECONDS with the extra arguments. The frames land in $work/user/Screenshots.
town() {
  local seconds="$1" shot="$2"
  shift 2
  rm -rf "$work/user"
  mkdir -p "$work/user/Save"
  local wheel=() i
  for ((i = 0; i < ${ZOOM:-28}; i++)); do wheel+=(--wheel "8@$(echo "16 + $i * 0.2" | bc)"); done
  BANISHED_USER_DIR="$work/user" BANISHED_SETTINGS="$work/settings.txt" \
    "$game" --boot-capture "$shot" "$seconds" --capture-size 1280x720 \
    --script "load $save@4.5" --click-at "$click@14" --press buttonCenter@14.6 \
    --press buttonClose@15.2 "${wheel[@]}" "$@" >"$work/log.txt" 2>&1
  if grep -q 'not found at' "$work/log.txt"; then
    cat "$work/log.txt" >&2
    exit 1
  fi
}

# encode START COUNT FILTER CRF OUT: frames START.. of the last town run, as VP9 WebM.
encode() {
  ffmpeg -loglevel error -y -framerate 30 -start_number "$1" \
    -i "$work/user/Screenshots/Screenshot%d.bmp" -frames:v "$2" -vf "$3,format=yuv420p" \
    -c:v libvpx-vp9 -b:v 0 -crf "$4" -row-mt 1 -an "$out/$5.webm"
}

for name in "${names[@]}"; do
  case "$name" in
    chicken-walk)
      # One loop of the 2.47 s Walk clip, at 30 frames per second.
      mkdir -p "$work/chicken"
      for ((i = 0; i < 74; i++)); do
        rm -rf "$work/pose" && mkdir -p "$work/pose"
        VIEWER_POSE="Walk@$(echo "scale=4; $i / 30" | bc)" VIEWER_ANIMS=Chicken \
          "$viewer" --capture "$work/pose" 'Animals\Chicken\ChickenMesh' >/dev/null 2>&1
        mv "$work/pose/Models_Animals_Chicken_ChickenMesh_resource.png" "$work/chicken/$(printf %03d "$i").png"
      done
      ffmpeg -loglevel error -y -framerate 30 -i "$work/chicken/%03d.png" \
        -vf "scale=384:384,format=yuv420p" -c:v libvpx-vp9 -b:v 0 -crf 34 -row-mt 1 -an \
        "$out/chicken-walk.webm"
      ;;
    citizens-market | zoom-into-town)
      # One run makes both videos.
      [[ -z "${market_done:-}" ]] || continue
      market_done=1
      # Every frame from 14 s is drawn, so the camera moves as in the windowed game.
      frames 14 "$(echo "scale=8; 1/30" | bc)" 600
      town 34.2 "$work/last.png" "${shots[@]}"
      # Frames 45..299: the zoom from above the town to the market (after the window closed).
      encode 45 255 "scale=768:432:flags=lanczos" 42 zoom-into-town
      # Frames 360..599 (26 to 34 s): citizens at the market, the middle of the frame.
      encode 360 240 "crop=960:540:160:90" 36 citizens-market
      ;;
    seasons)
      # Autumn to spring at speed 10: one frame every 0.75 s of capture time, from 20 to 200 s.
      frames 20 0.75 240
      ZOOM=16 town 200.5 "$work/last.png" --script "speed 10@15.5" "${shots[@]}"
      encode 0 240 "scale=768:432:flags=lanczos" 40 seasons
      ;;
    *)
      echo "unknown video: $name" >&2
      exit 2
      ;;
  esac
done
ls -la "$out"
