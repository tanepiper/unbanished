#!/usr/bin/env bash
# Regenerate the README / setup-guide screenshots in docs/assets/screenshots/.
#
#   tools/screenshots.sh SAVE.sav
#
# SAVE.sav is a town saved without mods (the town shots load it). Needs game-data/ (see
# tools/extract-assets.sh), a release build and `cwebp`. The captures use a scratch user folder
# and settings file, so the player's own saves and settings are never touched.
set -euo pipefail

repo="$(cd "$(dirname "$0")/.." && pwd)"
save="${1:?usage: tools/screenshots.sh SAVE.sav}"
out="$repo/docs/assets/screenshots"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

mkdir -p "$work/user/Save" "$out"
cp "$save" "$work/user/Save/"
export BANISHED_USER_DIR="$work/user" BANISHED_SETTINGS="$work/settings.txt"

cargo build -q --release -p banished-app --manifest-path "$repo/Cargo.toml"
game="$repo/target/release/banished"
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
