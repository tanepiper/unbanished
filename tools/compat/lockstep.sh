#!/usr/bin/env bash
# Lockstep in one process (docs/MULTIPLAYER-PLAN.md, step 3): record the `actions` scenario, then
# play its commands, split between players, in lockstep games joined by a simulated network with
# delay, jitter and busy computers (`unbanished --lockstep-test`). Every setting must keep the
# players identical; one player with no delay must also match the recording (a replay).
#
#   tools/compat/lockstep.sh BINARY
#
# Settings tried: players, input delay (updates), jitter (ticks), stall (% of ticks), network seed,
# digest checks (every N updates, 0 none) and a corruption (PLAYER:UPDATE, - for none). A run with
# a corruption must find the desync, resync the players from the host's save and end identical.
set -euo pipefail
# No run is ever heard (crates/audio keeps the volume at 0).
export UNBANISHED_MUTE=1
repo="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo"
source tools/compat/run-name.sh
arg="${1:?usage: tools/compat/lockstep.sh BINARY}"
if [[ -x "$arg" && -f "$arg" ]]; then bin="$arg"; else bin="$(tools/compat/build.sh "$arg")"; fi
out="target/compat/lockstep/$(run_name "$bin")"
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
  # Builds before the rename to unbanished read BANISHED_*, later ones UNBANISHED_*: set both,
  # so an old build never falls back to the player's own settings and saves.
  UNBANISHED_USER_DIR="$out/user" BANISHED_USER_DIR="$out/user" UNBANISHED_SETTINGS="$out/settings.txt" BANISHED_SETTINGS="$out/settings.txt" \
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
# A separate towns game (docs/SEPARATE-TOWNS.md): two towns on one map, made by this build,
# with its owners sidecar beside the save, and a recording of per-player commands, each acting
# on its own town (stock limits, notify flags, town names drawn from the shared Random).
UNBANISHED_SEPARATE_TOWNS=2 tools/compat/scenarios.sh "$bin" "$out/towns" new-game >/dev/null 2>&1 || true
towns_rec="$out/towns.rec"
cat > "$towns_rec" <<'REC'
{"start":{"engine":"unbanished tools/compat/lockstep.sh"}}
{"command":{"update":2,"player":0,"command":{"SetStockLimit":{"slot":3,"limit":42}}}}
{"command":{"update":10,"player":1,"command":{"SetStockLimit":{"slot":4,"limit":7}}}}
{"command":{"update":40,"player":1,"command":{"SetNotify":{"category":0,"on":false}}}}
{"command":{"update":80,"player":0,"command":{"RandomTownName":{"max_length":20}}}}
{"command":{"update":160,"player":1,"command":{"SetStockLimit":{"slot":3,"limit":100}}}}
{"command":{"update":240,"player":0,"command":{"SetSpeed":3}}}
{"command":{"update":400,"player":1,"command":{"RandomTownName":{"max_length":10}}}}
{"command":{"update":600,"player":0,"command":{"SetStockLimit":{"slot":5,"limit":9}}}}
REC
if [[ -f "$out/towns/new-game.sav" && -f "$out/towns/new-game.owners" ]]; then
  for s in "2 6 8 20 1 120 -" "2 4 15 30 3 60 1:400"; do
    read -r players delay jitter stall seed check corrupt <<<"$s"
    corruption=()
    [[ $corrupt == - ]] || corruption=("$corrupt")
    log="$out/towns-$players-$delay-$jitter-$stall-$seed-$check-${corrupt/:/@}"
    UNBANISHED_USER_DIR="$out/user" UNBANISHED_SETTINGS="$out/settings.txt" \
      ${limit[@]+"${limit[@]}"} "$bin" --lockstep-test "$out/towns/new-game.sav" "$towns_rec" \
      "$players" "$delay" "$jitter" "$stall" "$seed" "$check" ${corruption[@]+"${corruption[@]}"} \
      "$repo/game-data/original/WinData" >"$log.out" 2>"$log.log" || true
    result="$(tail -1 "$log.out")"
    resyncs="$(grep -o 'resyncs at \[[^]]*\]' "$log.out" || true)"
    echo "  separate towns, players $players, delay $delay, jitter $jitter, stall $stall%, seed $seed, checks $check, corrupt $corrupt: $result; $resyncs"
    [[ $result == identical:* ]] || failed=1
    if [[ $corrupt == - ]]; then
      [[ $resyncs == "resyncs at []" ]] || failed=1
    else
      [[ $resyncs != "resyncs at []" ]] || failed=1
    fi
  done
else
  echo "  separate towns: NO SAVE (see $out/towns)"
  failed=1
fi
if [[ $failed == 0 ]]; then
  echo "lockstep identical: $(basename "$bin")"
  rm -rf "$out"
else
  echo "LOCKSTEP DIFFERENT: see $out"
fi
exit $failed
