#!/usr/bin/env bash
# Make the short WebM videos of the documentation in docs/assets/videos/.
#
#   tools/videos.sh SAVE.sav [NAME...]      # NAME: chicken-walk citizens-market zoom-into-town seasons
#   tools/videos.sh save-game/Save83.sav montage   # the home page's 65 s montage (five saves, see below)
#   tools/videos.sh LARGE.sav big-town      # a large town, the camera circling its church
#   tools/videos.sh SAVE.sav orbit follow-citizen   # the camera circles the town; it follows a citizen
#   tools/videos.sh RIVER.sav river fishing  # along a river from a bridge; fishers on their dock
#   tools/videos.sh "save-game/Save 47.sav" boat-shoulder fisher-closeup river-flight
#                                           # the cinematic camera (--camera, --camera-follow)
#   tools/videos.sh save-game/Save83.sav farmer labourer vendor   # people at work, close up
#   FILTER=sepia tools/videos.sh SAVE.sav orbit     # also: grey, psychedelic (adds -FILTER to the name)
#   tools/videos.sh SAVE.sav filters        # orbit.webm (make it first) in all four looks, 2x2
#
# SAVE.sav is a town saved without mods; the town videos load it. They were first made from a
# 116-citizen town in late summer (Save83). With another save, set CLICK=X,Y to a building in the
# town centre (pixels in a 1280x720 frame, whatever the capture size): the camera centres on it
# before it zooms in.
#
# How it works (see docs/develop/VIDEOS.md): the game runs headless (--boot-capture) and a
# `screenshot` script command is queued for each video frame. Every command writes the next
# Screenshots/Screenshot<N>.bmp in the scratch user folder, and FFmpeg joins them. The chicken is
# posed by the viewer (VIEWER_POSE), one frame per run. Needs game-data/ (tools/extract-assets.sh),
# ffmpeg with libvpx, bc, and cwebp for the posters. The player's own saves and settings are never
# touched.
#
# Every town video is drawn at 1920x1080 with the highest video settings (HQ below) and scaled
# down; each has a NAME-poster.webp, the frame a page shows until the video plays. The cinematic
# videos (boat-shoulder, fisher-closeup, river-flight) use the cinematic camera, which changes
# only what is drawn. Their keyframes and times are for `Save 47` (a river town whose trader boat
# is on its way in when the save loads); with another save, change them.
#
# The montage is made only when asked for. It loads five saves from the folder of SAVE.sav (the
# names and the buildings it clicks are in the montage below): a river town (Save 47), a
# 116-citizen town (Save83), a large autumn town (Save200), a young town (Save5) and a winter town
# (Save1). With other saves, change them.
set -euo pipefail

repo="$(cd "$(dirname "$0")/.." && pwd)"
save="$(cd "$(dirname "${1:?usage: tools/videos.sh SAVE.sav [NAME...]}")" && pwd)/$(basename "$1")"
shift
names=("$@")
[[ ${#names[@]} -gt 0 ]] || names=(chicken-walk citizens-market zoom-into-town seasons)
out="$repo/docs/assets/videos"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
# MONTAGE_CLIPS=DIR keeps the montage's clips there, and reuses them when they are all there (to
# change the titles or the joins without drawing the shots again).
clips_dir="${MONTAGE_CLIPS:-$work/clips}"
mkdir -p "$out" "$clips_dir"

# GAME and VIEWER take binaries that are already built (for example from tools/compat/build.sh).
if [[ -z "${GAME:-}" || -z "${VIEWER:-}" ]]; then
  cargo build -q --release -p unbanished-app -p unbanished-viewer --manifest-path "$repo/Cargo.toml"
fi
game="${GAME:-$repo/target/release/unbanished}"
viewer="${VIEWER:-$repo/target/release/unbanished-viewer}"
click="${CLICK:-1060,470}"
# FFmpeg: FFMPEG if set, else Homebrew's ffmpeg-full (keg-only; it has drawtext for titles), else
# the ffmpeg on the PATH.
full="$(brew --prefix ffmpeg-full 2>/dev/null || true)"
ffmpeg="${FFMPEG:-$([[ -x $full/bin/ffmpeg ]] && echo "$full/bin/ffmpeg" || echo ffmpeg)}"
ffprobe="$(dirname "$(command -v "$ffmpeg")")/ffprobe"
# The montage's titles (drawtext): TITLE_FONT and CAPTION_FONT are font files; by default Georgia
# Bold and Georgia Italic (macOS), else fontconfig's serif. TITLES=0 leaves them out.
font() {
  if [[ -f $1 ]]; then echo "fontfile='$1'"; else echo "font='$2'"; fi
}
title_font="$(font "${TITLE_FONT:-/System/Library/Fonts/Supplemental/Georgia Bold.ttf}" "serif:style=Bold")"
caption_font="$(font "${CAPTION_FONT:-/System/Library/Fonts/Supplemental/Georgia Italic.ttf}" "serif:style=Italic")"

# fading FROM TO: an alpha expression that fades in over 0.6 s from FROM and out by TO (seconds).
fading() {
  echo "if(lt(t,$1),0,if(lt(t,$1+0.6),(t-$1)/0.6,if(lt(t,$2-0.6),1,if(lt(t,$2),($2-t)/0.6,0))))"
}

# text TEXT FONT SIZE X Y FROM TO [MORE]: a drawtext filter for TEXT (written to a file, so that
# no character needs escaping), white with a soft shadow, shown from FROM to TO.
text() {
  local file="$work/text-$RANDOM$RANDOM.txt"
  printf '%s' "$1" >"$file"
  echo "drawtext=$2:textfile='$file':fontsize=$3:fontcolor=white:x=$4:y=$5:shadowcolor=black@0.75:shadowx=2:shadowy=2:alpha='$(fading "$6" "$7")'${8:+:$8}"
}
# The frames are drawn at this size; positions on the screen are given for 1280x720 and scaled.
capture=1920x1080
# The highest video settings: 4x antialiasing, 16x anisotropic filtering, 2048 shadow maps, high
# shadows, reflections of everything.
HQ=(--setting VideoMultisample=3 --setting VideoTextureFilter=16 --setting VideoShadowScalar=0
  --setting VideoShaderDetail=3 --setting VideoReflectionDetail=2)
# FILTER=sepia|grey|psychedelic: a colour filter for every video made (FFmpeg, after the game).
case "${FILTER:-}" in
  "") look="" ;;
  sepia) look=",colorchannelmixer=.393:.769:.189:0:.349:.686:.168:0:.272:.534:.131" ;;
  grey) look=",hue=s=0" ;;
  psychedelic) look=",hue=h=t*180:s=3,eq=contrast=1.25:saturation=1.5" ;;
  *) echo "unknown FILTER: $FILTER (sepia, grey or psychedelic)" >&2; exit 2 ;;
esac
step="$(echo "scale=8; 1/30" | bc)"
# The middle three quarters of the frame, and its top three quarters (citizens look larger).
middle="crop=iw*3/4:ih*3/4:iw/8:ih/8"
top="crop=iw*3/4:ih*3/4:iw/8:0"

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

# px X,Y[@T]: a position on a 1280x720 screen, scaled to the capture size.
px() {
  local xy="${1%%@*}" rest=""
  [[ $1 == *@* ]] && rest="@${1#*@}"
  local f
  f="$(echo "scale=6; ${capture%x*} / 1280" | bc)"
  printf '%d,%d%s' "$(echo "${xy%,*} * $f / 1" | bc)" "$(echo "${xy#*,} * $f / 1" | bc)" "$rest"
}

# town SECONDS [ARGS...]: load the save, centre the camera on the building at $click (none: keep
# the saved view), zoom in, and run until SECONDS with the extra arguments, drawn with HQ. The
# frames land in $work/user/Screenshots. --move and --click-at positions are scaled (px).
town() {
  local seconds="$1"
  shift
  rm -rf "$work/user"
  mkdir -p "$work/user/Save"
  local wheel=() centre=() args=() i scale=""
  for ((i = 0; i < ${ZOOM:-28}; i++)); do wheel+=(--wheel "8@$(echo "16 + $i * ${ZSTEP:-0.2}" | bc)"); done
  [[ $click == none ]] || centre=(--click-at "$(px "$click@14")" --press buttonCenter@14.6 --press buttonClose@15.2)
  for a in "$@"; do
    if [[ -n $scale ]]; then a="$(px "$a")"; fi
    scale=""
    [[ $a == --move || $a == --click-at ]] && scale=1
    args+=("$a")
  done
  UNBANISHED_USER_DIR="$work/user" UNBANISHED_SETTINGS="$work/settings.txt" \
    "$game" --boot-capture "$work/last.png" "$seconds" --capture-size "$capture" "${HQ[@]}" \
    --script "load $save@4.5" ${centre[@]+"${centre[@]}"} ${wheel[@]+"${wheel[@]}"} \
    ${args[@]+"${args[@]}"} >"$work/log.txt" 2>&1
  if grep -q 'not found at' "$work/log.txt"; then
    cat "$work/log.txt" >&2
    exit 1
  fi
}

# encode START COUNT FILTER CRF OUT: frames START.. of the last town run, as VP9 WebM.
encode() {
  "$ffmpeg" -loglevel error -y -framerate 30 -start_number "$1" \
    -i "$work/user/Screenshots/Screenshot%d.bmp" -frames:v "$2" -vf "$3$look,format=yuv420p" \
    -c:v libvpx-vp9 -b:v 0 -crf "$4" -row-mt 1 -an "$out/$5${FILTER:+-$FILTER}.webm"
}

# poster FRAME OUT [CROP]: frame FRAME of the last town run as OUT-poster.webp (768x432), the
# picture a page shows until its video plays.
poster() {
  "$ffmpeg" -loglevel error -y -i "$work/user/Screenshots/Screenshot$1.bmp" \
    -vf "${3:+$3,}scale=768:432:flags=lanczos$look" "$work/poster.png"
  cwebp -quiet -q 75 "$work/poster.png" -o "$out/$2${FILTER:+-$FILTER}-poster.webp"
}

# clip NAME START COUNT [CROP]: frames START.. of the last town run as a montage clip (lossless
# enough H.264 at 1280x720).
clip() {
  "$ffmpeg" -loglevel error -y -framerate 30 -start_number "$2" \
    -i "$work/user/Screenshots/Screenshot%d.bmp" -frames:v "$3" \
    -vf "${4:+$4,}scale=1280:720:flags=lanczos,format=yuv420p" -c:v libx264 -crf 12 \
    "$clips_dir/$1.mp4"
}

# The runs. Each sets its save and camera, and leaves its frames for encode, poster or clip.

# The market of Save83: every frame from 14 s to 34 s, as the camera zooms down to it.
run_market() {
  frames 14 "$step" 600
  town 34.2 "${shots[@]}"
}

# A year at speed 10 from late summer, one frame every STEP seconds of capture time.
run_seasons() {
  frames 20 "$1" "$2"
  ZOOM=16 town "$(echo "20.5 + $1 * $2" | bc)" --script "speed 10@15.5" "${shots[@]}"
}

# Over the trader's shoulder (Save 47), 10 s from 19.5 s: the camera rides 6 behind and 3.4 above
# the boat, looks 30 ahead, and keeps the bridges out of the way on its own.
run_boat() {
  frames 19.5 "$step" 300
  click=none ZOOM=0 town 29.9 --camera-follow "boat 0@5" --camera-rig 6,3.4,0,30,-1 \
    --camera-fov 50 "${shots[@]}"
}

# A fisher at the end of the dock (Save 47): the camera circles from behind them to their face in
# 10 s (three keyframes round the fisher at 54.55, 21.6).
run_fisher() {
  frames 30 "$step" 300
  click=none ZOOM=0 town 40.4 \
    --camera "57.1,18.5,2.1>54.55,21.6,0.7@30" --camera "58.5,21.9,1.9>54.55,21.6,0.7@35" \
    --camera "56.5,25.1,2.0>54.55,21.6,0.7@40" "${shots[@]}"
}

# Down the river into the town of Save 47, past the bridges and the trading post, then up over the
# town: six keyframes in 10 s (the river's line is the boat's route).
run_flight() {
  frames 20 "$step" 300
  click=none ZOOM=0 town 30.4 \
    --camera "-75,74,24>-15,70,0@20" --camera "-30,80,20>25,52,0@22" \
    --camera "10,66,17>62,20,0@24" --camera "48,38,17>82,-4,0@26" \
    --camera "74,18,22>80,-40,0@28" --camera "105,-20,45>70,-20,0@30" "${shots[@]}"
}

# run_follow SUBJECT FIRST SECONDS: a close-up of a --camera-follow subject at speed 2, from FIRST
# for SECONDS.
run_follow() {
  frames "$2" "$step" "$(($3 * 30))"
  click=none ZOOM=0 town "$(echo "$2 + $3 + 0.4" | bc)" --script "speed 2@6" \
    --camera-follow "$1@5" --camera-rig 3.5,2.6,0.8,3,1.2 --camera-fov 45 "${shots[@]}"
}

# The farmer (harvesting the field by the market, then carrying the crop), the labourer (past the
# log and stone stockpile) and the vendor (among the market stalls) of Save83.
run_farmer() { run_follow "profession farm 0" 15 8; }
run_labourer() { run_follow "profession labor 1" 10 8; }
run_vendor() { run_follow "profession vendor 0" 21 8; }

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
      "$ffmpeg" -loglevel error -y -framerate 30 -i "$work/chicken/%03d.png" \
        -vf "scale=384:384,format=yuv420p" -c:v libvpx-vp9 -b:v 0 -crf 34 -row-mt 1 -an \
        "$out/chicken-walk.webm"
      ;;
    citizens-market | zoom-into-town)
      # One run makes both videos.
      [[ -z "${market_done:-}" ]] || continue
      market_done=1
      run_market
      # Frames 45..299: the zoom from above the town to the market (after the window closed).
      encode 45 255 "scale=768:432:flags=lanczos" 42 zoom-into-town
      poster 200 zoom-into-town
      # Frames 360..599 (26 to 34 s): citizens at the market, the middle of the frame.
      encode 360 240 "$middle,scale=960:540:flags=lanczos" 36 citizens-market
      poster 420 citizens-market "$middle"
      ;;
    seasons)
      # Autumn to spring at speed 10: one frame every 0.75 s of capture time, from 20 to 200 s.
      run_seasons 0.75 240
      encode 0 240 "scale=768:432:flags=lanczos" 40 seasons
      poster 150 seasons
      ;;
    big-town)
      # A large town (Save200, Driscola) at normal speed, so that no smoke or weather drifts
      # into view: centred on the church by the autumn woods (CLICK, in 1280x720 screen
      # coordinates of the saved view), zoomed in part way, then circling it for 10 s. The
      # middle of the frame leaves out the toolbar.
      frames 21 "$step" 300
      click="${CLICK:-1000,480}" ZOOM=14 town 31.5 --hold RotateRight@21-31.2 "${shots[@]}"
      encode 0 300 "$middle,scale=1280:720:flags=lanczos" 52 big-town
      poster 90 big-town "$middle"
      ;;
    orbit)
      # Centre on the building at $click, zoom in part way, then circle it for 10 s.
      frames 21 "$step" 300
      ZOOM=20 town 31.5 --hold RotateRight@21-31.2 --hold PitchUp@21-21.6 "${shots[@]}"
      encode 0 300 "scale=768:432:flags=lanczos" 47 orbit
      poster 150 orbit
      ;;
    follow-citizen)
      # Select a citizen (CITIZEN, the n-th living one), press Follow, zoom in and watch them
      # for 10 s. The citizen window closes; the screenshots have no interface.
      frames 23 "$step" 300
      click=none ZOOM=30 town 33.5 --script "select citizen ${CITIZEN:-5}@14" \
        --press buttonFollow@14.5 "${shots[@]}"
      encode 0 300 "scale=854:480:flags=lanczos" 40 follow-citizen
      poster 150 follow-citizen
      ;;
    river)
      # Centre on a bridge (CLICK, default for Save 47), lower the camera, and fly along the
      # river for 7 s.
      frames 21 "$step" 210
      click="${CLICK:-965,232}" ZOOM=18 town 28.5 --hold PitchUp@20-20.8 --hold Back@21-28.2 "${shots[@]}"
      encode 0 210 "scale=768:432:flags=lanczos" 46 river
      poster 100 river
      ;;
    fishing)
      # Centre on a fishing dock (CLICK, default for Save 47), zoom in close and turn slowly
      # around it for 10 s, at speed 3 so that the fishers come and go.
      frames 23 "$step" 300
      click="${CLICK:-912,275}" ZOOM=34 town 33.5 --script "speed 3@15.5" \
        --hold PitchUp@22-22.6 --hold RotateLeft@23-27 "${shots[@]}"
      encode 0 300 "scale=768:432:flags=lanczos" 44 fishing
      poster 150 fishing
      ;;
    boat-shoulder)
      run_boat
      encode 0 300 "scale=768:432:flags=lanczos" 40 boat-shoulder
      poster 150 boat-shoulder
      ;;
    fisher-closeup)
      run_fisher
      encode 0 300 "scale=768:432:flags=lanczos" 41 fisher-closeup
      poster 240 fisher-closeup
      ;;
    river-flight)
      run_flight
      encode 0 300 "scale=768:432:flags=lanczos" 44 river-flight
      poster 150 river-flight
      ;;
    farmer | labourer | vendor)
      "run_$name"
      encode 0 240 "scale=768:432:flags=lanczos" 42 "$name"
      poster "$([[ $name == farmer ]] && echo 60 || echo 120)" "$name"
      ;;
    filters)
      # The orbit video in its four looks side by side: as drawn, sepia, grey and psychedelic
      # (the FILTER colours above), labelled when FFmpeg has drawtext. Make orbit first.
      src="$out/orbit.webm"
      [[ -f $src ]] || { echo "filters: make orbit first ($src)" >&2; exit 1; }
      label() { echo ""; }
      if [[ $("$ffmpeg" -hide_banner -filters 2>/dev/null) == *" drawtext "* ]]; then
        label() {
          echo ",drawtext=$caption_font:text='$1':fontsize=18:fontcolor=white:x=12:y=h-th-10:shadowcolor=black@0.75:shadowx=2:shadowy=2"
        }
      fi
      "$ffmpeg" -loglevel error -y -i "$src" -filter_complex \
        "[0]trim=0:7,setpts=PTS-STARTPTS,scale=384:216:flags=lanczos,split=4[a][b][c][d];\
[a]null$(label 'As drawn')[a2];\
[b]colorchannelmixer=.393:.769:.189:0:.349:.686:.168:0:.272:.534:.131$(label 'Sepia')[b2];\
[c]hue=s=0$(label 'Grey')[c2];\
[d]hue=h=t*180:s=3,eq=contrast=1.25:saturation=1.5$(label 'Psychedelic')[d2];\
[a2][b2]hstack[top];[c2][d2]hstack[bottom];[top][bottom]vstack,format=yuv420p" \
        -c:v libvpx-vp9 -b:v 0 -crf 55 -row-mt 1 -an "$out/filters.webm"
      "$ffmpeg" -loglevel error -y -ss 4 -i "$out/filters.webm" -frames:v 1 "$work/poster.png"
      cwebp -quiet -q 75 "$work/poster.png" -o "$out/filters-poster.webp"
      ;;
    montage)
      # The home page's video, about 65 s: twelve shots from five saves, joined with 0.6 s
      # transitions of different kinds. Each shot is a clip of a run above, or a game-camera run
      # of its own (save, building to centre on, wheel steps, extra arguments).
      dir="$(dirname "$save")"
      s47="$dir/Save 47.sav" s83="$dir/Save83.sav"
      clips=(flight boat fisher market farmer vendor labourer autumn workers winter seasons outro)
      have=0
      for c in "${clips[@]}"; do [[ -f $clips_dir/$c.mp4 ]] && have=$((have + 1)); done
      if ((have < ${#clips[@]})); then
      save="$s47" run_flight && clip flight 0 270
      save="$s47" run_boat && clip boat 30 180
      save="$s47" run_fisher && clip fisher 60 150
      save="$s83" run_market && clip market 360 150 "$middle"
      save="$s83" run_farmer && clip farmer 0 150
      save="$s83" run_vendor && clip vendor 30 150
      save="$s83" run_labourer && clip labourer 90 120
      frames 14 "$step" 400
      save="$dir/Save200.sav" click=950,350 ZOOM=26 town 27.6 --move 640,360@19 --move 0,360@21.4 "${shots[@]}"
      clip autumn 225 165
      save="$dir/Save5.sav" click=740,355 ZOOM=28 town 27.6 --move 640,360@19 --move 0,360@21.4 \
        --move 640,360@22.5 "${shots[@]}"
      clip workers 225 165 "$top"
      save="$dir/Save1.sav" click=650,425 ZOOM=26 town 27.6 --move 640,360@19 --move 0,719@21.4 "${shots[@]}"
      clip winter 225 165
      # A year at speed 10, from late summer: one frame every 1.35 s of capture time, 9 s long.
      save="$s83" click=1060,470 run_seasons 1.35 270 && clip seasons 0 270
      # The camera rises from the river town and turns away.
      out_wheel=()
      for ((i = 0; i < 28; i++)); do out_wheel+=(--wheel "-8@$(echo "23 + $i * 0.2" | bc)"); done
      frames 14 "$step" 450
      save="$s47" click=600,270 town 29.3 "${out_wheel[@]}" --hold RotateLeft@23-29 "${shots[@]}"
      clip outro 270 180
      fi
      # The caption of each shot (the first has the title, the last the closing card).
      captions=("" "The trader rows in" "A fisher at the end of the dock" "Market day" "Harvest"
        "At the vendor’s stall" "Logs for the stockpile" "Autumn" "A young town" "Winter"
        "A year in nine seconds" "")
      joins=(fadeblack smoothleft circleopen dissolve slideleft wipeleft radial fade smoothup hblur fadewhite)
      inputs=() graph="" prev=v0 offset=0 length=0 starts=(0)
      for i in "${!clips[@]}"; do
        inputs+=(-i "$clips_dir/${clips[$i]}.mp4")
        graph+="[$i:v]scale=854:480:flags=lanczos,fps=30,format=yuv420p,setpts=PTS-STARTPTS[v$i];"
      done
      for ((i = 1; i < ${#clips[@]}; i++)); do
        length="$("$ffprobe" -v error -show_entries format=duration -of csv=p=0 "$clips_dir/${clips[$((i - 1))]}.mp4")"
        offset=$(echo "$offset + $length - 0.6" | bc)
        starts+=("$offset")
        graph+="[$prev][v$i]xfade=transition=${joins[$((i - 1))]}:duration=0.6:offset=$offset[x$i];"
        prev="x$i"
      done
      end="$(echo "$offset + 6" | bc)"
      titles=""
      filters="$("$ffmpeg" -hide_banner -filters 2>/dev/null || true)"
      if [[ ${TITLES:-1} != 0 && $filters == *" drawtext "* ]]; then
        # The opening title over the flight, a caption low on the left for each shot, and a
        # closing card over the outro.
        titles=",$(text "Un-Banished" "$title_font" 64 "(w-tw)/2" "h*0.36" 0.6 5.5)"
        titles+=",$(text "Banished, rebuilt in Rust – native and in the browser" "$caption_font" 24 "(w-tw)/2" "h*0.36+82" 1.2 5.5)"
        for i in "${!clips[@]}"; do
          [[ -n ${captions[$i]} ]] || continue
          titles+=",$(text "${captions[$i]}" "$caption_font" 24 36 "h-64" "$(echo "${starts[$i]} + 0.8" | bc)" \
            "$(echo "${starts[$((i + 1))]} - 0.1" | bc)" "box=1:boxcolor=black@0.35:boxborderw=10")"
        done
        titles+=",$(text "Un-Banished" "$title_font" 56 "(w-tw)/2" "h*0.38" "$(echo "$end - 4.4" | bc)" "$end")"
        titles+=",$(text "Recorded by the game itself, without a window" "$caption_font" 22 "(w-tw)/2" "h*0.38+74" "$(echo "$end - 3.8" | bc)" "$end")"
      elif [[ ${TITLES:-1} != 0 ]]; then
        echo "no drawtext in $ffmpeg: the montage has no titles (brew install ffmpeg-full)" >&2
      fi
      graph+="[$prev]fade=t=in:d=0.8,fade=t=out:st=$(echo "$end - 0.8" | bc):d=0.8$titles$look[out]"
      "$ffmpeg" -loglevel error -y "${inputs[@]}" -filter_complex "$graph" -map "[out]" \
        -c:v libvpx-vp9 -b:v 0 -crf 50 -row-mt 1 -an "$out/montage${FILTER:+-$FILTER}.webm"
      # The poster shown until the video loads: the frame at 3 s, under the title.
      "$ffmpeg" -loglevel error -y -ss 3 -i "$out/montage${FILTER:+-$FILTER}.webm" -frames:v 1 "$work/poster.png"
      cwebp -quiet -q 70 "$work/poster.png" -o "$out/montage${FILTER:+-$FILTER}-poster.webp"
      ;;
    *)
      echo "unknown video: $name" >&2
      exit 2
      ;;
  esac
done
ls -la "$out"
