#!/usr/bin/env bash
# Capture the port at a reference frame's resolution and compare them in one step.
#
# Usage:
#   tools/reference/compare.sh REF [--no-autocrop] [--ignore x,y,w,h]... -- SECONDS PORT-ARGS...
#     REF        reference id under game-data/reference (e.g. seed-999518904/start-t0)
#     SECONDS    simulated time of the port capture
#     PORT-ARGS  scripted boot-capture options (--press, --combo, --wheel, --script ...);
#                env such as UNBANISHED_SEED is passed through
#
# Black bars around the reference are removed first (a stream of the original at another aspect
# ratio); pass --no-autocrop for frames whose UI is legitimately black at the edges (main menu).
# Writes game-data/reference/compare/REF/{port.png,panels.png,heat.png,report.json} and prints
# the report. Example:
#   UNBANISHED_SEED=999518904 tools/reference/compare.sh seed-999518904/start-t0 \
#     --ignore 0,0,120,130 -- 16 --press buttonNew@4.5 --combo comboTerrain=Mountains@4.8 \
#     --combo comboTerrainSize=Small@4.8 --press buttonOk@5.0 --wheel -1@9 ...
set -euo pipefail
cd "$(dirname "$0")/../.."
ref="$1"; shift
autocrop=1
ignore=()
while [[ $# -gt 0 && "$1" != "--" ]]; do
  case "$1" in
    --no-autocrop) autocrop=0 ;;
    --ignore) ignore+=(--ignore "$2"); shift ;;
    *) echo "unknown option $1" >&2; exit 2 ;;
  esac
  shift
done
[[ "${1:-}" == "--" ]] && shift
secs="$1"; shift
src="game-data/reference/$ref.png"
out="game-data/reference/compare/$ref"
mkdir -p "$out"
cargo build -q --release -p unbanished-app --bin unbanished --example imgdiff
diff=./target/release/examples/imgdiff
if [[ $autocrop == 1 ]]; then
  crop="$($diff "$src" --print-crop)"
else
  size="$(sips -g pixelWidth -g pixelHeight "$src" | awk '/pixel/{print $2}' | paste -sd, -)"
  crop="0,0,$size"
fi
IFS=, read -r _ _ w h <<<"$crop"
./target/release/unbanished --boot-capture "$out/port.png" "$secs" --capture-size "${w}x${h}" "$@" \
  >"$out/port.log" 2>&1
$diff "$src" "$out/port.png" --crop-a "$crop" --blur 3 --threshold 32 ${ignore[@]+"${ignore[@]}"} \
  --out "$out/heat.png" --compose "$out/panels.png" --json | tee "$out/report.json"
echo "reference crop $crop; panels: $out/panels.png"
