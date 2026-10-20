#!/usr/bin/env bash
# Extract your own copy of Banished's game data into game-data/ (git-ignored).
#
#   tools/extract-assets.sh                 # auto-detect the Steam install
#   UNBANISHED_DIR=/path/to/Banished tools/extract-assets.sh
#
# UNBANISHED_DIR is the folder containing WinData/ and the original binaries.
set -euo pipefail

repo="$(cd "$(dirname "$0")/.." && pwd)"

candidates=(
  "${UNBANISHED_DIR:-}"
  "$HOME/Library/Application Support/Steam/steamapps/common/Banished"
  "$HOME/Library/Application Support/Steam/Steam.AppBundle/Steam/Contents/MacOS/steamapps/content/app_242920/depot_242921"
  "$HOME/.steam/steam/steamapps/common/Banished"
  "$HOME/.local/share/Steam/steamapps/common/Banished"
  "/c/Program Files (x86)/Steam/steamapps/common/Banished"
)

game=""
for dir in "${candidates[@]}"; do
  if [[ -n "$dir" && -f "$dir/WinData/data0.pkg" ]]; then game="$dir"; break; fi
done
if [[ -z "$game" ]]; then
  echo "Could not find Banished (looked for WinData/data0.pkg)." >&2
  echo "Set UNBANISHED_DIR to the game folder and try again." >&2
  exit 1
fi

out="$repo/game-data"
echo "Game:   $game"
echo "Output: $out"
mkdir -p "$out/original"

# Keep a pristine copy of the inputs so analysis does not depend on the Steam install.
for f in "$game"/*.exe "$game"/*.dll; do cp -p "$f" "$out/original/"; done
mkdir -p "$out/original/WinData"
cp -p "$game"/WinData/*.pkg "$out/original/WinData/"

rm -rf "$out/extracted"
# Lowest package priority first, so higher-priority packages overwrite shared names.
python3 -I "$repo/tools/pkg_extract.py" "$out/extracted" \
  "$out/original/WinData/data1.pkg" "$out/original/WinData/patch1.pkg" \
  "$out/original/WinData/patch0.pkg" "$out/original/WinData/data0.pkg" || {
  echo "Extraction finished with known-unsupported entries (see above)." >&2
}

(cd "$out/original" && shasum -a 256 *.exe *.dll WinData/*.pkg) > "$out/MANIFEST.sha256"
echo "Wrote $out/MANIFEST.sha256"

# Audio is xWMA, which browsers cannot play; decode it to MP3 when FFmpeg is available.
if command -v ffmpeg >/dev/null && command -v cargo >/dev/null; then
  (cd "$repo" && cargo run -q --release -p unbanished-cli -- export-audio) || echo "Audio export failed." >&2
else
  echo "Skipping audio export (needs ffmpeg and cargo)." >&2
fi
