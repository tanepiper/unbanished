#!/usr/bin/env bash
# File a screenshot of the original game (Steam Remote Play stream) as a reference frame.
#
# Usage: tools/reference/save.sh SOURCE.(jpg|png) NAME [NOTE...]
#   SOURCE  a computer-use screenshot of the stream window (its saved blob path) or any image
#   NAME    reference id, e.g. ui/main-menu, seed-1234/start-minimap (slashes make folders)
#
# Writes game-data/reference/NAME.png (converted with macOS sips; git-ignored, never commit)
# and NAME.txt (source, capture time, size, note). Compare against the port with
#   cargo run -q --release -p unbanished-app --example imgdiff -- \
#     game-data/reference/NAME.png port.png --resize --blur 2 [--crop-a ..] [--crop-b ..]
set -euo pipefail
# No run is ever heard (crates/audio keeps the volume at 0).
export UNBANISHED_MUTE=1
cd "$(dirname "$0")/../.."
src="$1"; name="$2"; shift 2
out="game-data/reference/$name"
mkdir -p "$(dirname "$out")"
sips -s format png "$src" --out "$out.png" >/dev/null
size="$(sips -g pixelWidth -g pixelHeight "$out.png" | awk '/pixel/{print $2}' | paste -sd x -)"
{
  echo "source: $src"
  echo "captured: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "size: $size"
  echo "note: $*"
} > "$out.txt"
echo "$out.png ($size)"
