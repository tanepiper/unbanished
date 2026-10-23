#!/usr/bin/env bash
# Run a web app locally. Serves your git-ignored game-data/ folder on 127.0.0.1:8732 and the app via
# trunk's proxy: the viewer on http://127.0.0.1:8731/?data=/game-data/ (default) or, with
# `--app game`, the game on http://127.0.0.1:8733/?data=/game-data/. Never deploy this proxy.
set -euo pipefail
# No run is ever heard (crates/audio keeps the volume at 0).
export UNBANISHED_MUTE=1
repo="$(cd "$(dirname "$0")/.." && pwd)"
data="$repo/game-data/original/WinData"
[[ -f "$data/data0.pkg" ]] || { echo "Run tools/extract-assets.sh first." >&2; exit 1; }
python3 -m http.server 8732 --bind 127.0.0.1 --directory "$repo/game-data" >/dev/null 2>&1 &
server=$!
trap 'kill $server 2>/dev/null' EXIT
app=viewer
port=8731
if [[ "${1:-}" == "--app" ]]; then
  app="$2"
  shift 2
fi
[[ "$app" == game ]] && port=8733
cd "$repo/apps/$app"
# Separate target dir: avoids lock contention with other cargo builds in the workspace.
export CARGO_TARGET_DIR="$repo/target/web"
# `&mute` keeps the game silent (crates/audio, the web Device); drop it to hear the game.
echo "$app: http://127.0.0.1:$port/?data=/game-data/&mute"
trunk serve "$@"
