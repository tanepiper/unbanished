#!/usr/bin/env bash
# Lockstep in one process (docs/MULTIPLAYER-PLAN.md, step 3): record the `actions` scenario, then
# play its commands, split between players, in lockstep games joined by a simulated network with
# delay, jitter and busy computers (`banished --lockstep-test`). Every setting must keep the
# players identical; one player with no delay must also match the recording (a replay).
#
#   tools/compat/lockstep.sh BINARY
#
# Settings tried: players, input delay (updates), jitter (ticks), stall (% of ticks), network seed.
set -euo pipefail
repo="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo"
arg="${1:?usage: tools/compat/lockstep.sh BINARY}"
if [[ -x "$arg" && -f "$arg" ]]; then bin="$arg"; else bin="$(tools/compat/build.sh "$arg")"; fi
out="target/compat/lockstep/$(basename "$bin")"
rm -rf "$out" && mkdir -p "$out/recordings"
RECORD_DIR="$(cd "$out/recordings" && pwd)" tools/compat/scenarios.sh "$bin" "$out/runs" actions
limit=()
if command -v gtimeout >/dev/null; then limit=(gtimeout 1800); fi
settings=(
  "1 0 0 0 1"    # one player, no delay: the recording itself
  "2 0 0 0 1"    # two players, no delay: they wait for each other every update
  "3 6 8 20 1"   # three players, delay 6, turns 1-9 ticks late, busy 20% of ticks
  "3 6 8 20 7"   # the same with another network seed
  "4 4 30 10 3"  # four players, jitter far beyond the delay: they stall and catch up
  "2 10 2 60 5"  # two players, long delay, busy 60% of ticks
)
failed=0
for s in "${settings[@]}"; do
  read -r players delay jitter stall seed <<<"$s"
  result="$(BANISHED_USER_DIR="$out/user" BANISHED_SETTINGS="$out/settings.txt" \
    ${limit[@]+"${limit[@]}"} "$bin" --lockstep-test save-game/Save83.sav "$out/recordings/actions.rec" \
    "$players" "$delay" "$jitter" "$stall" "$seed" "$repo/game-data/original/WinData" \
    2>"$out/$players-$delay-$jitter-$stall-$seed.log" | tail -1)" || true
  echo "  players $players, delay $delay, jitter $jitter, stall $stall%, seed $seed: $result"
  [[ $result == identical:* ]] || failed=1
done
if [[ $failed == 0 ]]; then
  echo "lockstep identical: $(basename "$bin")"
  rm -rf "$out"
else
  echo "LOCKSTEP DIFFERENT: see $out"
fi
exit $failed
