#!/usr/bin/env bash
# Make the short WebM videos of the documentation in docs/assets/videos/.
#
#   tools/videos.sh SAVE.sav [NAME...]      # NAME: chicken-walk citizens-market zoom-into-town seasons
#   tools/videos.sh SAVE.sav montage        # the home page's 39 s montage (five saves, see below)
#   tools/videos.sh LARGE.sav big-town      # a large town at speed 10, the camera turning over it
#   tools/videos.sh SAVE.sav orbit follow-citizen   # the camera circles the town; it follows a citizen
#   tools/videos.sh RIVER.sav river fishing  # along a river from a bridge; fishers on their dock
#
# SAVE.sav is a town saved without mods; the town videos load it. They were first made from a
# 116-citizen town in late summer. With another save, set CLICK=X,Y to a building in the town
# centre (pixels in a 1280x720 frame): the camera centres on it before it zooms in.
#
# How it works (see docs/develop/VIDEOS.md): the game runs headless (--boot-capture) and a
# `screenshot` script command is queued for each video frame. Every command writes the next
# Screenshots/Screenshot<N>.bmp in the scratch user folder, and FFmpeg joins them. The chicken is
# posed by the viewer (VIEWER_POSE), one frame per run. Needs game-data/ (tools/extract-assets.sh),
# ffmpeg with libvpx, bc, and cwebp for the montage's poster. The player's own saves and settings are never touched.
#
# The montage is made only when asked for. It loads five saves from the folder of SAVE.sav (the
# names and the buildings it clicks are in MONTAGE below): a river town, a 116-citizen town, a
# large autumn town, a young town and a winter town. With other saves, change MONTAGE.
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

# town SECONDS OUT [ARGS...]: load the save, centre the camera on the building at $click (none:
# keep the saved view), zoom in, and run until SECONDS with the extra arguments. The frames land
# in $work/user/Screenshots.
town() {
  local seconds="$1" shot="$2"
  shift 2
  rm -rf "$work/user"
  mkdir -p "$work/user/Save"
  local wheel=() centre=() i
  for ((i = 0; i < ${ZOOM:-28}; i++)); do wheel+=(--wheel "8@$(echo "16 + $i * ${ZSTEP:-0.2}" | bc)"); done
  [[ $click == none ]] || centre=(--click-at "$click@14" --press buttonCenter@14.6 --press buttonClose@15.2)
  BANISHED_USER_DIR="$work/user" BANISHED_SETTINGS="$work/settings.txt" \
    "$game" --boot-capture "$shot" "$seconds" --capture-size 1280x720 \
    --script "load $save@4.5" ${centre[@]+"${centre[@]}"} ${wheel[@]+"${wheel[@]}"} "$@" >"$work/log.txt" 2>&1
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
    big-town)
      # A large town (the 0.2.0 notes use a 1,024-citizen save) at speed 10, from the saved
      # view: 8 s of frames from 17 s while the camera turns slowly to the left.
      frames 17 "$(echo "scale=8; 1/30" | bc)" 240
      click=none ZOOM=0 town 25.5 "$work/last.png" --script "speed 10@15.5" \
        --hold RotateLeft@17-25.2 "${shots[@]}"
      encode 0 240 "scale=854:480:flags=lanczos" 42 big-town
      ;;
    orbit)
      # Centre on the building at $click, zoom in part way, then circle it for 10 s.
      frames 21 "$(echo "scale=8; 1/30" | bc)" 300
      ZOOM=20 town 31.5 "$work/last.png" --hold RotateRight@21-31.2 --hold PitchUp@21-21.6 "${shots[@]}"
      encode 0 300 "scale=768:432:flags=lanczos" 47 orbit
      ;;
    follow-citizen)
      # Select a citizen (CITIZEN, the n-th living one), press Follow, zoom in and watch them
      # for 10 s. The citizen window closes; the screenshots have no interface.
      frames 23 "$(echo "scale=8; 1/30" | bc)" 300
      click=none ZOOM=30 town 33.5 "$work/last.png" --script "select citizen ${CITIZEN:-5}@14" \
        --press buttonFollow@14.5 "${shots[@]}"
      encode 0 300 "scale=854:480:flags=lanczos" 40 follow-citizen
      ;;
    river)
      # Centre on a bridge (CLICK, default for the river town of the montage), lower the
      # camera, and fly along the river for 10 s.
      frames 21 "$(echo "scale=8; 1/30" | bc)" 210
      click="${CLICK:-965,232}" ZOOM=18 town 28.5 "$work/last.png" --hold PitchUp@20-20.8 \
        --hold Back@21-28.2 "${shots[@]}"
      encode 0 210 "scale=768:432:flags=lanczos" 46 river
      ;;
    fishing)
      # Centre on a fishing dock (CLICK), zoom in close and turn slowly around it for 10 s,
      # at speed 3 so that the fishers come and go.
      frames 23 "$(echo "scale=8; 1/30" | bc)" 300
      click="${CLICK:-912,275}" ZOOM=34 town 33.5 "$work/last.png" --script "speed 3@15.5" \
        --hold PitchUp@22-22.6 --hold RotateLeft@23-27 "${shots[@]}"
      encode 0 300 "scale=768:432:flags=lanczos" 44 fishing
      ;;
    montage)
      # Seven shots, joined with 0.5 s cross-fades: name, save, building to centre on, wheel
      # steps, first frame (s), last frame (s), extra arguments. Every frame from 14 s is drawn,
      # so the camera eases as in the windowed game. --move to a screen edge scrolls the camera.
      dir="$(dirname "$save")"
      out_wheel=()
      for ((i = 0; i < 28; i++)); do out_wheel+=(--wheel "-8@$(echo "23 + $i * 0.2" | bc)"); done
      MONTAGE=(
        "descend|Save 47|600,270|28|15.9|21.5|"
        "market|Save83|1060,470|28|25|30.6|"
        "autumn|Save200|950,350|26|21.5|27|--move 640,360@19 --move 0,360@21.4"
        "workers|Save5|740,355|28|21.5|27|--move 640,360@19 --move 0,360@21.4 --move 640,360@22.5"
        "winter|Save1|650,425|26|21.5|27|--move 640,360@19 --move 0,719@21.4"
        "outro|Save 47|600,270|28|23|28.6|OUT"
      )
      for shot in "${MONTAGE[@]}"; do
        IFS='|' read -r clip file click_at zoom first last extra <<<"$shot"
        [[ $extra == OUT ]] && extra_args=("${out_wheel[@]}") || read -ra extra_args <<<"$extra"
        frames 14 "$(echo "scale=8; 1/30" | bc)" "$(echo "($last - 14) * 30 / 1" | bc)"
        [[ $clip == descend ]] && zstep=0.18 || zstep=0.2
        save="$dir/$file.sav" click="$click_at" ZOOM="$zoom" ZSTEP="$zstep" \
          town "$(echo "$last + 0.3" | bc)" "$work/last.png" ${extra_args[@]+"${extra_args[@]}"} "${shots[@]}"
        ffmpeg -loglevel error -y -framerate 30 -start_number "$(echo "($first - 14) * 30 / 1" | bc)" \
          -i "$work/user/Screenshots/Screenshot%d.bmp" -frames:v 165 -c:v libx264 -crf 12 \
          -pix_fmt yuv420p "$work/$clip.mp4"
      done
      # A year at speed 10, from late summer: one frame every 1.35 s of capture time, 9 s long.
      frames 20 1.35 270
      save="$dir/Save83.sav" click=1060,470 ZOOM=16 town 385 "$work/last.png" \
        --script "speed 10@15.5" "${shots[@]}"
      ffmpeg -loglevel error -y -framerate 30 -i "$work/user/Screenshots/Screenshot%d.bmp" \
        -c:v libx264 -crf 12 -pix_fmt yuv420p "$work/seasons.mp4"
      # Join: the market and the young town are cropped to the middle and the top of the frame.
      clips=(descend market autumn workers winter seasons outro)
      crops=("" "crop=960:540:160:90," "" "crop=960:540:160:0," "" "" "")
      lengths=(5.5 5.5 5.5 5.5 5.5 9 5.5)
      inputs=() graph="" prev=v0 offset=0
      for i in "${!clips[@]}"; do
        inputs+=(-i "$work/${clips[$i]}.mp4")
        graph+="[$i:v]${crops[$i]}scale=854:480:flags=lanczos,fps=30,format=yuv420p,setpts=PTS-STARTPTS[v$i];"
      done
      for ((i = 1; i < ${#clips[@]}; i++)); do
        offset=$(echo "$offset + ${lengths[$((i - 1))]} - 0.5" | bc)
        graph+="[$prev][v$i]xfade=transition=fade:duration=0.5:offset=$offset[x$i];"
        prev="x$i"
      done
      ffmpeg -loglevel error -y "${inputs[@]}" -filter_complex "${graph%;}" -map "[$prev]" \
        -c:v libvpx-vp9 -b:v 0 -crf 46 -row-mt 1 -an "$out/montage.webm"
      # The poster shown until the video loads: the frame at 4 s.
      ffmpeg -loglevel error -y -ss 4 -i "$out/montage.webm" -frames:v 1 "$work/poster.png"
      cwebp -quiet -q 70 "$work/poster.png" -o "$out/montage-poster.webp"
      ;;
    *)
      echo "unknown video: $name" >&2
      exit 2
      ;;
  esac
done
ls -la "$out"
