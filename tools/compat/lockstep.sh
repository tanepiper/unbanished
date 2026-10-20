#!/usr/bin/env bash
# Lockstep in one process (docs/MULTIPLAYER-PLAN.md, step 3): record the `actions` scenario, then
# play its commands, split between players, in lockstep games joined by a simulated network with
# delay, jitter and busy computers (`banished --lockstep-test`). Every setting must keep the
# players identical; one player with no delay must also match the recording (a replay).
#
#   tools/compat/lockstep.sh BINARY
#
# Settings tried: players, input delay (updates), jitter (ticks), stall (% of ticks), network seed,
# digest checks (every N updates, 0 none) and a corruption (PLAYER:UPDATE, - for none). A run with
# a corruption must find the desync, resync the players from the host's save and end identical.
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
  "1 0 0 0 1 600 -"      # one player, no delay: the recording itself
  "2 0 0 0 1 600 -"      # two players, no delay: they wait for each other every update
  "3 6 8 20 1 120 -"     # three players, delay 6, turns 1-9 ticks late, busy 20% of ticks
  "3 6 8 20 7 120 -"     # the same with another network seed
  "4 4 30 10 3 600 -"    # four players, jitter far beyond the delay: they stall and catch up
  "2 10 2 60 5 600 -"    # two players, long delay, busy 60% of ticks
  "3 6 8 20 1 120 1:400" # player 1's game scrambled at 400: found, resynced
  "4 4 30 10 3 60 3:1000" # player 3's at 1000, with jitter beyond the delay
)
failed=0
for s in "${settings[@]}"; do
  read -r players delay jitter stall seed check corrupt <<<"$s"
  corruption=()
  [[ $corrupt == - ]] || corruption=("$corrupt")
  log="$out/$players-$delay-$jitter-$stall-$seed-$check-${corrupt/:/@}"
  BANISHED_USER_DIR="$out/user" BANISHED_SETTINGS="$out/settings.txt" \
    ${limit[@]+"${limit[@]}"} "$bin" --lockstep-test save-game/Save83.sav "$out/recordings/actions.rec" \
    "$players" "$delay" "$jitter" "$stall" "$seed" "$check" ${corruption[@]+"${corruption[@]}"} \
    "$repo/game-data/original/WinData" >"$log.out" 2>"$log.log" || true
  result="$(tail -1 "$log.out")"
  resyncs="$(grep -o 'resyncs at \[[^]]*\]' "$log.out" || true)"
  echo "  players $players, delay $delay, jitter $jitter, stall $stall%, seed $seed, checks $check, corrupt $corrupt: $result; $resyncs"
  [[ $result == identical:* ]] || failed=1
  # A corruption must have been resynced; without one there must be no resync.
  if [[ $corrupt == - ]]; then
    [[ $resyncs == "resyncs at []" ]] || failed=1
  else
    [[ $resyncs != "resyncs at []" ]] || failed=1
  fi
done
if [[ $failed == 0 ]]; then
  echo "lockstep identical: $(basename "$bin")"
  rm -rf "$out"
else
  echo "LOCKSTEP DIFFERENT: see $out"
fi
exit $failed
