#!/usr/bin/env bash
# Record the compatibility scenarios that load a save, then replay each recording from its save
# without a window: every recorded digest of the game state must match (`banished --replay`).
# This shows that the player commands carry everything a player changes
# (docs/MULTIPLAYER-PLAN.md, step 2).
#
#   tools/compat/replay.sh BINARY [SCENARIO...]     # default: small-town medium-town large-town actions
#
# BINARY is a game binary (or a git revision, built clean with tools/compat/build.sh). Exit 0 when
# every replay matches. Results stay in target/compat/replays/<binary>.
set -euo pipefail
repo="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo"
arg="${1:?usage: tools/compat/replay.sh BINARY [SCENARIO...]}"
shift
scenarios=("$@")
[[ ${#scenarios[@]} -eq 0 ]] && scenarios=(small-town medium-town large-town actions)
if [[ -x "$arg" && -f "$arg" ]]; then bin="$arg"; else bin="$(tools/compat/build.sh "$arg")"; fi
out="target/compat/replays/$(basename "$bin")"
rm -rf "$out" && mkdir -p "$out/recordings"
RECORD_DIR="$(cd "$out/recordings" && pwd)" tools/compat/scenarios.sh "$bin" "$out/runs" "${scenarios[@]}"
limit=()
if command -v gtimeout >/dev/null; then limit=(gtimeout 900); fi
save_of() {
  case "$1" in
    small-town) echo save-game/Save0.sav ;;
    medium-town | actions) echo save-game/Save83.sav ;;
    large-town) echo save-game/Auto-1.sav ;;
  esac
}
failed=0
for s in "${scenarios[@]}"; do
  rec="$out/recordings/$s.rec"
  if [[ ! -f "$rec" ]]; then
    echo "  $s: no recording"
    failed=1
    continue
  fi
  verdict="$(BANISHED_USER_DIR="$out/user" BANISHED_SETTINGS="$out/settings.txt" \
    ${limit[@]+"${limit[@]}"} "$bin" --replay "$(save_of "$s")" "$rec" "$repo/game-data/original/WinData" \
    2>"$out/$s.replay.log" | tail -1)" || true
  echo "  $s: $verdict"
  [[ $verdict == identical:* ]] || failed=1
done
if [[ $failed == 0 ]]; then
  echo "replays identical: $(basename "$bin")"
  rm -rf "$out"
else
  echo "REPLAY DIFFERENT: see $out"
fi
exit $failed
