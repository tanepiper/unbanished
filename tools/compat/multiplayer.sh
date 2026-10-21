#!/usr/bin/env bash
# The Multiplayer window from end to end (docs/MULTIPLAYER-PLAN.md, step 6): a relay and headless
# games kept to the clock (`--realtime`). The host opens the window, hosts a room, waits for the
# guests, presses Start and picks its save; each guest types the room code and joins (again while
# the room is not open yet). A guest changes the speed during the game, a shared command. The host
# checks the players' digests every 60 updates; every check must agree, and there must be enough
# of them.
#
#   two:   the host and one guest.
#   leave: the host and two guests; the second guest quits during the game. The host and the
#          other guest go on without it and still agree.
#
#   tools/compat/multiplayer.sh BINARY [RELAY_BINARY] [SCENARIO...]   # default: both scenarios
#
# Exit 0 when the players agree. Results and frames stay in target/compat/multiplayer/<binary>.
set -euo pipefail
repo="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo"
bin="${1:?usage: tools/compat/multiplayer.sh BINARY [RELAY_BINARY]}"
relay="${2:-target/release/unbanished-server}"
# The default server is built from this checkout, so it never runs a stale one.
if [[ -z "${2:-}" ]]; then cargo build -q --offline --release -p unbanished-server; fi
scenarios=("${@:3}")
((${#scenarios[@]})) || scenarios=(two leave)
root="target/compat/multiplayer/$(basename "$bin")"
rm -rf "$root" && mkdir -p "$root"
limit=()
if command -v gtimeout >/dev/null; then limit=(gtimeout 300); fi
port=$((7900 + RANDOM % 100))
address="ws://127.0.0.1:$port"
"$relay" "127.0.0.1:$port" >"$root/relay.log" 2>&1 &
relay_pid=$!
trap 'kill $relay_pid 2>/dev/null || true' EXIT
sleep 1
capture=(--capture-size 960x540 --realtime)
failed=0

host() { # dir room seconds
  # Builds before the rename to unbanished read BANISHED_*, later ones UNBANISHED_*: set both,
  # so an old build never falls back to the player's own settings and saves.
  UNBANISHED_ROOM="$2" BANISHED_ROOM="$2" UNBANISHED_NET_CHECK=60 BANISHED_NET_CHECK=60 UNBANISHED_USER_DIR="$1/host" BANISHED_USER_DIR="$1/host" \
    UNBANISHED_SETTINGS="$1/host.txt" BANISHED_SETTINGS="$1/host.txt" ${limit[@]+"${limit[@]}"} "$bin" \
    --boot-capture "$1/host.png" "$3" "${capture[@]}" \
    --press buttonMultiplayer@4.5 --text "numberSeed=$address@4.7" --press buttonOk@5 \
    --press buttonOk@14 --press buttonLoad@17 >"$1/host.out" 2>"$1/host.log"
}
guest() { # dir name room seconds [option...]
  local dir="$1" name="$2" room="$3" seconds="$4"
  shift 4
  UNBANISHED_USER_DIR="$dir/$name" BANISHED_USER_DIR="$dir/$name" UNBANISHED_SETTINGS="$dir/$name.txt" BANISHED_SETTINGS="$dir/$name.txt" ${limit[@]+"${limit[@]}"} \
    "$bin" --boot-capture "$dir/$name.png" "$seconds" "${capture[@]}" \
    --press buttonMultiplayer@6 --text "numberSeed=$address@6.2" --text "editName=$room@6.5" \
    --press buttonOk@7 --press buttonOk@9 --press buttonOk@11 "$@" \
    >"$dir/$name.out" 2>"$dir/$name.log"
}
count() { grep -c "$1" "$2" || true; }

run() { # two | leave
  local name="$1" dir="$root/$1" room ok=1 pids=()
  room="$(echo "$name" | tr '[:lower:]' '[:upper:]')"
  mkdir -p "$dir/host/Save" "$dir/guest1" "$dir/guest2"
  cp save-game/Save83.sav "$dir/host/Save/"
  host "$dir" "$room" 50 &
  pids+=($!)
  guest "$dir" guest1 "$room" 50 --script "speed 5@30" &
  pids+=($!)
  if [[ $name == leave ]]; then
    guest "$dir" guest2 "$room" 30 &
    pids+=($!)
  fi
  for pid in "${pids[@]}"; do wait "$pid" || ok=0; done
  local agree differ started summary
  agree="$(count "the players agree" "$dir/host.log")"
  differ="$(count "the players differ" "$dir/host.log")"
  started="$(count " starts" "$dir/guest1.log")"
  ((started == 1 && agree >= 10 && differ == 0)) || ok=0
  summary="guest started: $started; checks agreed: $agree; differed: $differ"
  if [[ $name == leave ]]; then
    local left after
    left="$(count "left; the game goes on" "$dir/host.log")"
    # Checks after the departure: the two who stayed kept playing together.
    after="$(sed -n '/left; the game goes on/,$p' "$dir/host.log" | grep -c "the players agree" || true)"
    ((left == 1 && after >= 5)) || ok=0
    summary+="; departures: $left; checks agreed after it: $after"
  fi
  echo "  $name: $summary: $([[ $ok == 1 ]] && echo identical || echo FAILED)"
  [[ $ok == 1 ]] || failed=1
}

for s in "${scenarios[@]}"; do run "$s"; done
if ((failed == 0)); then
  echo "multiplayer identical: $(basename "$bin")"
else
  echo "multiplayer FAILED: $(basename "$bin") (see $root)"
  exit 1
fi
