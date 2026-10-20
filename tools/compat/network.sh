#!/usr/bin/env bash
# Multiplayer over a real network (docs/MULTIPLAYER-PLAN.md, step 5): start a relay
# (banished-relay) on this computer, record the `actions` scenario, and run several game
# processes as the players of one room (`banished --net-test`). The host starts the game from its
# save; the others get it over the network. Each plays its share of the recording; their digests
# must be identical. A second run scrambles player 1's game: the host must find it and resync.
#
#   tools/compat/network.sh BINARY [RELAY_BINARY]     # default relay: target/release/banished-relay
#
# Exit 0 when every run agrees. Results stay in target/compat/network/<binary> on failure.
set -euo pipefail
repo="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo"
bin="${1:?usage: tools/compat/network.sh BINARY [RELAY_BINARY]}"
relay="${2:-target/release/banished-relay}"
out="target/compat/network/$(basename "$bin")"
rm -rf "$out" && mkdir -p "$out/recordings"
RECORD_DIR="$(cd "$out/recordings" && pwd)" tools/compat/scenarios.sh "$bin" "$out/runs" actions
recording="$out/recordings/actions.rec"
limit=()
if command -v gtimeout >/dev/null; then limit=(gtimeout 900); fi
port=$((7800 + RANDOM % 100))
"$relay" "127.0.0.1:$port" >"$out/relay.log" 2>&1 &
relay_pid=$!
trap 'kill $relay_pid 2>/dev/null || true' EXIT
sleep 1
failed=0
run() { # name players corrupt-player corrupt-update
  local name="$1" players="$2" who="$3" at="$4" p pids=()
  for ((p = 0; p < players; p++)); do
    local role=join extra=()
    ((p == 0)) && role=host
    ((p == who)) && extra=("$at")
    BANISHED_USER_DIR="$out/$name/user$p" BANISHED_SETTINGS="$out/$name/settings$p.txt" \
      ${limit[@]+"${limit[@]}"} "$bin" --net-test "ws://127.0.0.1:$port" "$role" "$name" \
      save-game/Save83.sav "$recording" "$players" 6 120 ${extra[@]+"${extra[@]}"} \
      "$repo/game-data/original/WinData" >"$out/$name/p$p.out" 2>"$out/$name/p$p.log" &
    pids+=($!)
    ((p == 0)) && sleep 1
  done
  local ok=1
  for pid in "${pids[@]}"; do wait "$pid" || ok=0; done
  # After the host's last resync (none: from the start), every player's digests match.
  local after resyncs
  resyncs="$(tail -1 "$out/$name/p0.out")"
  after="$(sed -n 's/.*resyncs \[\(.*\)\]/\1/p' "$out/$name/p0.out" | tr ',' '\n' | tail -1 | tr -d ' ')"
  after="${after:-0}"
  # Each player's digests after it, the last line of each update (a player that was ahead at a
  # resync prints some updates again).
  digests() { awk -v a="$after" '$1 == "update" && $2 > a { last[$2] = $0 } END { for (u in last) print last[u] }' "$1" | sort -n -k2; }
  for ((p = 1; p < players; p++)); do
    diff <(digests "$out/$name/p0.out") <(digests "$out/$name/p$p.out") >/dev/null || ok=0
  done
  local count
  count="$(digests "$out/$name/p0.out" | wc -l | tr -d ' ')"
  # A scrambled player must have been resynced; otherwise there must be none.
  if ((who >= 0)); then [[ $resyncs != *"resyncs []"* ]] || ok=0; else [[ $resyncs == *"resyncs []"* ]] || ok=0; fi
  echo "  $name: $players players, $count digests compared after update $after; $resyncs: $([[ $ok == 1 ]] && echo identical || echo DIFFERENT)"
  [[ $ok == 1 ]] || failed=1
}
mkdir -p "$out/three" "$out/scrambled"
run three 3 -1 0
run scrambled 3 1 400
if [[ $failed == 0 ]]; then
  echo "network identical: $(basename "$bin")"
  rm -rf "$out"
else
  echo "NETWORK DIFFERENT: see $out"
fi
exit $failed
