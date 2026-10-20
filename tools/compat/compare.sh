#!/usr/bin/env bash
# Compare two versions of the game: they must play the same scenarios identically.
#
#   tools/compat/compare.sh BASE NEW [SCENARIO...]
#
# BASE and NEW are git revisions (built clean with tools/compat/build.sh) or paths to game
# binaries. The script:
#   1. runs the scenarios (tools/compat/scenarios.sh) with BASE twice, to prove the scenarios
#      are deterministic on this machine;
#   2. runs them with NEW and compares each save's game state (`savegame_same`) and frame
#      (`imgdiff`) with BASE's;
#   3. checks backward compatibility: BASE and NEW both continue each save BASE wrote, and the
#      results must again be identical.
# Exit 0 when everything matches. Results stay in target/compat/runs/<base>-<new>.
#
# Example: tools/compat/compare.sh v0.2.0 HEAD
set -euo pipefail
repo="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo"
base_arg="${1:?usage: tools/compat/compare.sh BASE NEW [SCENARIO...]}"
new_arg="${2:?usage: tools/compat/compare.sh BASE NEW [SCENARIO...]}"
shift 2

binary() {
  if [[ -x "$1" && -f "$1" ]]; then
    echo "$1"
  else
    tools/compat/build.sh "$1"
  fi
}
base="$(binary "$base_arg")"
new="$(binary "$new_arg")"
runs="target/compat/runs/$(basename "$base")-vs-$(basename "$new")"
rm -rf "$runs" && mkdir -p "$runs"
cargo build -q --offline --release -p unbanished-app --example imgdiff
cargo build -q --offline --release -p unbanished-formats --example savegame_same
failed=0

same() { # label dirA dirB
  local label="$1" a="$2" b="$3"
  for sav in "$a"/*.sav; do
    local name
    name="$(basename "$sav" .sav)"
    local verdict="identical"
    if [[ ! -f "$b/$name.sav" ]]; then
      verdict="missing in $b"
    elif ! ./target/release/examples/savegame_same "$sav" "$b/$name.sav" >/dev/null; then
      verdict="DIFFERENT game state"
    elif ! ./target/release/examples/imgdiff "$a/$name.png" "$b/$name.png" --json |
      grep -q '"max_delta":0'; then
      verdict="DIFFERENT frame"
    fi
    echo "  $label $name: $verdict"
    [[ "$verdict" == identical ]] || failed=1
  done
}

echo "1. $base_arg against itself"
tools/compat/scenarios.sh "$base" "$runs/base-1" "$@"
tools/compat/scenarios.sh "$base" "$runs/base-2" "$@"
same "base/base" "$runs/base-1" "$runs/base-2"
echo "2. $new_arg against $base_arg"
tools/compat/scenarios.sh "$new" "$runs/new" "$@"
same "base/new" "$runs/base-1" "$runs/new"
echo "3. both continue $base_arg's saves"
continues=()
for sav in "$runs/base-1"/*.sav; do continues+=("continue:$sav"); done
tools/compat/scenarios.sh "$base" "$runs/continue-base" "${continues[@]}"
tools/compat/scenarios.sh "$new" "$runs/continue-new" "${continues[@]}"
same "continue" "$runs/continue-base" "$runs/continue-new"

if [[ $failed == 0 ]]; then
  echo "compatible: $new_arg plays every scenario exactly as $base_arg"
  rm -rf "$runs"
else
  echo "NOT COMPATIBLE: see $runs (tools: savegame_fields_diff, imgdiff)"
fi
exit $failed
