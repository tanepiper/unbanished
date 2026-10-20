#!/usr/bin/env bash
# The Multiplayer window from end to end (docs/MULTIPLAYER-PLAN.md, step 6): a relay and two
# headless games kept to the clock (`--realtime`). The host opens the window, hosts a room, waits
# for the guest, presses Start and picks its save; the guest types the room code and joins (again
# while the room is not open yet). The guest changes the speed during the game, a shared command.
# The host checks the players' digests every 60 updates; every check must agree, and there must
# be enough of them.
#
#   tools/compat/multiplayer.sh BINARY [RELAY_BINARY]     # default relay: target/release/banished-relay
#
# Exit 0 when the players agree. Results and frames stay in target/compat/multiplayer/<binary>.
set -euo pipefail
repo="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo"
bin="${1:?usage: tools/compat/multiplayer.sh BINARY [RELAY_BINARY]}"
relay="${2:-target/release/banished-relay}"
out="target/compat/multiplayer/$(basename "$bin")"
rm -rf "$out" && mkdir -p "$out/host/Save" "$out/guest"
cp save-game/Save83.sav "$out/host/Save/"
limit=()
if command -v gtimeout >/dev/null; then limit=(gtimeout 300); fi
port=$((7900 + RANDOM % 100))
address="ws://127.0.0.1:$port"
"$relay" "127.0.0.1:$port" >"$out/relay.log" 2>&1 &
relay_pid=$!
trap 'kill $relay_pid 2>/dev/null || true' EXIT
sleep 1
capture=(--capture-size 960x540 --realtime)
BANISHED_ROOM=TEST BANISHED_NET_CHECK=60 BANISHED_USER_DIR="$out/host" \
  BANISHED_SETTINGS="$out/host.txt" ${limit[@]+"${limit[@]}"} "$bin" \
  --boot-capture "$out/host.png" 45 "${capture[@]}" \
  --press buttonMultiplayer@4.5 --text "numberSeed=$address@4.7" --press buttonOk@5 \
  --press buttonOk@14 --press buttonLoad@17 >"$out/host.out" 2>"$out/host.log" &
host=$!
BANISHED_USER_DIR="$out/guest" BANISHED_SETTINGS="$out/guest.txt" ${limit[@]+"${limit[@]}"} "$bin" \
  --boot-capture "$out/guest.png" 45 "${capture[@]}" \
  --press buttonMultiplayer@6 --text "numberSeed=$address@6.2" --text editName=TEST@6.5 \
  --press buttonOk@7 --press buttonOk@9 --press buttonOk@11 --script "speed 5@30" \
  >"$out/guest.out" 2>"$out/guest.log" &
guest=$!
ok=1
wait "$host" || ok=0
wait "$guest" || ok=0
agree="$(grep -c "the players agree" "$out/host.log" || true)"
differ="$(grep -c "the players differ" "$out/host.log" || true)"
started="$(grep -c "player 1 of 2 starts" "$out/guest.log" || true)"
echo "  guest started: $started; checks agreed: $agree; differed: $differ"
((started == 1 && agree >= 10 && differ == 0)) || ok=0
if ((ok == 1)); then
  echo "multiplayer identical: $(basename "$bin")"
else
  echo "multiplayer FAILED: $(basename "$bin") (see $out)"
  exit 1
fi
