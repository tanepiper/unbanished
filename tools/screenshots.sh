#!/usr/bin/env bash
# Regenerate the README / setup-guide screenshots in docs/assets/screenshots/.
#
#   tools/screenshots.sh SAVE.sav
#   tools/screenshots.sh --visual-change BASE NEW [SCENARIO]
#
# SAVE.sav is a town saved without mods (the town shots load it). Needs game-data/ (see
# tools/extract-assets.sh), a release build and `cwebp`. The captures use a scratch user folder
# and settings file, so the player's own saves and settings are never touched.
#
# --visual-change makes the example pictures of docs/develop/VISUAL-CHANGES.md in
# docs/assets/screenshots/visual-change/. BASE and NEW are git revisions (built with
# tools/compat/build.sh) or game binaries. It plays SCENARIO (default small-town, see
# tools/compat/scenarios.sh) twice with BASE and once with NEW, then writes the imgdiff heat
# maps of BASE against itself and against NEW, and for each CROPS entry ("name:WxH+X+Y", default
# the rocks and ore of the small town) the region before, after and in the heat map, side by
# side and enlarged without smoothing. Needs ImageMagick (`magick`) as well.
set -euo pipefail
# No run is ever heard (crates/audio keeps the volume at 0).
export UNBANISHED_MUTE=1

repo="$(cd "$(dirname "$0")/.." && pwd)"

if [[ "${1:-}" == "--visual-change" ]]; then
  binary() { if [[ -x "$1" && -f "$1" ]]; then echo "$1"; else "$repo/tools/compat/build.sh" "$1"; fi; }
  base="$(binary "${2:?usage: tools/screenshots.sh --visual-change BASE NEW [SCENARIO]}")"
  new="$(binary "${3:?usage: tools/screenshots.sh --visual-change BASE NEW [SCENARIO]}")"
  scenario="${4:-small-town}"
  crops="${CROPS:-rocks:240x160+960+250 ore:240x160+220+350}"
  out="$repo/docs/assets/screenshots/visual-change"
  work="$(mktemp -d)"
  trap 'rm -rf "$work"' EXIT
  mkdir -p "$out"
  for run in base-1:"$base" base-2:"$base" new:"$new"; do
    "$repo/tools/compat/scenarios.sh" "${run#*:}" "$work/${run%%:*}" "$scenario"
  done
  cargo build -q --release -p unbanished-app --example imgdiff --manifest-path "$repo/Cargo.toml"
  imgdiff="$repo/target/release/examples/imgdiff"
  a="$work/base-1/$scenario.png" b="$work/new/$scenario.png"
  "$imgdiff" "$a" "$work/base-2/$scenario.png" --out "$work/heat-base.png" --json && echo
  "$imgdiff" "$a" "$b" --out "$work/heat.png" --json && echo
  for name in heat-base heat; do
    cwebp -quiet -q 75 -resize 1000 0 "$work/$name.png" -o "$out/$name.webp"
  done
  for crop in $crops; do
    name="${crop%%:*}" region="${crop#*:}"
    parts=()
    for img in "$a" "$b" "$work/heat.png"; do
      part="$work/part-${#parts[@]}.png"
      parts+=("$part")
      magick "$img" -crop "$region" +repage -filter point -resize 200% "$part"
    done
    magick "${parts[@]}" -bordercolor white -border 4 +append "$work/$name.png"
    cwebp -quiet -q 80 "$work/$name.png" -o "$out/$name.webp"
  done
  ls -la "$out"
  exit 0
fi
save="${1:?usage: tools/screenshots.sh SAVE.sav}"
out="$repo/docs/assets/screenshots"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

mkdir -p "$work/user/Save" "$out"
cp "$save" "$work/user/Save/"
export UNBANISHED_USER_DIR="$work/user" UNBANISHED_SETTINGS="$work/settings.txt"

cargo build -q --release -p unbanished-app --manifest-path "$repo/Cargo.toml"
game="$repo/target/release/unbanished"
size=(--capture-size 1600x900)
load=(--press buttonLoad@4.5 --press buttonLoad@5.5)

"$game" --boot-capture "$work/main-menu.png" 4.5 "${size[@]}"
"$game" --boot-capture "$work/new-game.png" 16 "${size[@]}" --press buttonNew@4.5 --press buttonOk@5.0
"$game" --boot-capture "$work/town-overview.png" 25 "${size[@]}" "${load[@]}"
# The click lands on the Town Hall in the save these were first made from; adjust for another.
"$game" --boot-capture "$work/town-hall.png" 22 "${size[@]}" "${load[@]}" --click-at "${CLICK:-1078,560}@19"

for name in main-menu new-game town-overview town-hall; do
  cwebp -quiet -q 80 -resize 1280 0 "$work/$name.png" -o "$out/$name.webp"
done
ls -la "$out"
