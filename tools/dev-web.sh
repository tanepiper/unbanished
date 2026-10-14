#!/usr/bin/env bash
# Run the web viewer locally. Serves your git-ignored game-data/ folder on 127.0.0.1:8732 and the
# viewer on http://127.0.0.1:8731/?data=/game-data/ via trunk's proxy. Never deploy this proxy.
set -euo pipefail
repo="$(cd "$(dirname "$0")/.." && pwd)"
data="$repo/game-data/original/WinData"
[[ -f "$data/data0.pkg" ]] || { echo "Run tools/extract-assets.sh first." >&2; exit 1; }
python3 -m http.server 8732 --bind 127.0.0.1 --directory "$repo/game-data" >/dev/null 2>&1 &
server=$!
trap 'kill $server 2>/dev/null' EXIT
cd "$repo/apps/viewer"
# Separate target dir: avoids lock contention with other cargo builds in the workspace.
export CARGO_TARGET_DIR="$repo/target/web"
echo "Viewer: http://127.0.0.1:8731/?data=/game-data/"
trunk serve "$@"
