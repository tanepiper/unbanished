#!/usr/bin/env bash
# The multiplayer server from end to end (docs/ARCHITECTURE-PLAN.md, phase D): a server that keeps
# rooms, and headless players (`unbanished --net-test`).
#
#   1. play: three players. Player 2's game is scrambled at update 400 and the server resyncs it;
#      player 1 leaves at update 900; a new player joins once the host is past update 1000.
#      Every player's digests after the last resync are the host's, and the server keeps the
#      room's town in its folder.
#   2. restart: the server stops and starts again on the same folder. The host opens the room
#      again with --resume and goes on from the kept town with a second player; their digests
#      agree.
#
#   tools/compat/server.sh BINARY [SERVER_BINARY] [--simulate]
#
# The default server is built from this checkout. With --simulate the server runs each room's
# game itself from this checkout's game data (its `simulate` setting): a first run with no
# scramble must agree with the server at every check, with no resync, and the scrambled player
# is resynced from the server's own save.
#
# Exit 0 when everything agrees. Results stay in target/compat/server/<binary>-<hash> when it does not.
set -euo pipefail
# No run is ever heard (crates/audio keeps the volume at 0).
export UNBANISHED_MUTE=1
repo="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo"
source tools/compat/run-name.sh
simulate=0
args=()
for a in "$@"; do
  if [[ $a == --simulate ]]; then simulate=1; else args+=("$a"); fi
done
bin="${args[0]:?usage: tools/compat/server.sh BINARY [SERVER_BINARY] [--simulate]}"
server="${args[1]:-target/release/unbanished-server}"
# The default server is built from this checkout, so it never runs a stale one.
if [[ -z "${args[1]:-}" ]]; then cargo build -q --offline --release -p unbanished-server; fi
suffix=""
if [[ $simulate == 1 ]]; then suffix="-simulate"; fi
out="target/compat/server/$(run_name "$bin")$suffix"
rm -rf "$out" && mkdir -p "$out/recordings" "$out/rooms"
RECORD_DIR="$(cd "$out/recordings" && pwd)" tools/compat/scenarios.sh "$bin" "$out/runs" actions
recording="$out/recordings/actions.rec"
limit=()
if command -v gtimeout >/dev/null; then limit=(gtimeout 900); fi
port=$((7900 + RANDOM % 100))
printf 'address = 127.0.0.1:%s\nrooms = %s\n' "$port" "$(cd "$out/rooms" && pwd)" >"$out/server.conf"
if [[ $simulate == 1 ]]; then
  printf 'simulate = %s\n' "$repo/game-data/original/WinData" >>"$out/server.conf"
fi
server_pid=
start_server() { # log
  "$server" --config "$out/server.conf" >"$1" 2>&1 &
  server_pid=$!
  sleep 1
}
trap 'kill $server_pid 2>/dev/null || true' EXIT
failed=0

# One player: dir number role players [extra --net-test arguments...]
player() {
  local dir="$1" p="$2" role="$3" players="$4"
  shift 4
  UNBANISHED_USER_DIR="$dir/user$p" BANISHED_USER_DIR="$dir/user$p" UNBANISHED_SETTINGS="$dir/settings$p.txt" BANISHED_SETTINGS="$dir/settings$p.txt" \
    ${limit[@]+"${limit[@]}"} "$bin" --net-test "ws://127.0.0.1:$port" "$role" ROOM1 \
    save-game/Save83.sav "$recording" "$players" 6 120 "$@" \
    "$repo/game-data/original/WinData" >"$dir/p$p.out" 2>"$dir/p$p.log" &
  pids+=($!)
}
# The last line of each update's digests.
digests() { awk -v a="${2:-0}" '$1 == "update" && $2 > a { last[$2] = $0 } END { for (u in last) print last[u] }' "$1" | sort -n -k2; }
# Every digest of player file $2 (after update $3) is the host's ($1).
same() { ! grep -Fxvq -f <(digests "$1" "$3") <(digests "$2" "$3"); }

# 0. With --simulate: two players and the server's own game agree at every check.
if [[ $simulate == 1 ]]; then
  dir="$out/agree" && mkdir -p "$dir" && pids=()
  start_server "$dir/server.log"
  player "$dir" 0 host 2
  sleep 1
  player "$dir" 1 join 2
  ok=1
  for pid in "${pids[@]}"; do wait "$pid" || ok=0; done
  same "$dir/p0.out" "$dir/p1.out" 0 || ok=0
  grep -q "the server runs the game" "$dir/server.log" || ok=0
  agreed="$(grep -c "the players agree" "$dir/server.log" || true)"
  ((agreed >= 5)) || ok=0
  grep -q "differ" "$dir/server.log" && ok=0
  grep -q "resyncs \[\]" "$dir/p0.out" || ok=0
  echo "  agree: the server's game and two players agreed at $agreed checks: $([[ $ok == 1 ]] && echo identical || echo DIFFERENT)"
  [[ $ok == 1 ]] || failed=1
  kill "$server_pid" 2>/dev/null || true
  wait "$server_pid" 2>/dev/null || true
fi

# 1. Play: a scramble, a departure, a new player.
dir="$out/play" && mkdir -p "$dir" && pids=()
start_server "$dir/server.log"
player "$dir" 0 host 3
sleep 1
player "$dir" 1 join 3 --leave-at 900
player "$dir" 2 join 3 400
waited=0
until grep -q '^update   1020' "$dir/p0.out" 2>/dev/null || ((waited > 900)); do
  sleep 0.1
  waited=$((waited + 1))
done
player "$dir" 3 join 3
ok=1
for pid in "${pids[@]}"; do wait "$pid" || ok=0; done
after="$(sed -n 's/.*resyncs \[\(.*\)\]/\1/p' "$dir/p0.out" | tr ',' '\n' | tail -1 | tr -d ' ')"
# With --simulate the server resyncs the players from its own game.
if [[ $simulate == 1 ]]; then
  after="$(sed -n "s/.*go on from the server's game at update \([0-9]*\).*/\1/p" "$dir/server.log" | tail -1)"
fi
for p in 1 2 3; do same "$dir/p0.out" "$dir/p$p.out" "${after:-0}" || ok=0; done
[[ -n "$after" ]] || ok=0 # the scrambled player was resynced
grep -q "leaves at update" "$dir/p1.log" || ok=0
[[ -s "$dir/p3.out" ]] && grep -q '^update' "$dir/p3.out" || ok=0
[[ -f "$out/rooms/ROOM1.sav" ]] || ok=0
kept="$(cat "$out/rooms/ROOM1.update" 2>/dev/null || echo none)"
echo "  play: resynced at ${after:-none}; player 1 left; a new player joined; town kept at update $kept: $([[ $ok == 1 ]] && echo identical || echo DIFFERENT)"
[[ $ok == 1 ]] || failed=1
kill "$server_pid" 2>/dev/null || true
wait "$server_pid" 2>/dev/null || true

# 2. Restart: the town goes on from the kept save.
dir="$out/restart" && mkdir -p "$dir" && pids=()
start_server "$dir/server.log"
player "$dir" 0 host 2 --resume
sleep 1
player "$dir" 1 join 2
ok=1
for pid in "${pids[@]}"; do wait "$pid" || ok=0; done
same "$dir/p0.out" "$dir/p1.out" 0 || ok=0
grep -q "resuming the room's town kept at update" "$dir/p0.log" || ok=0
count="$(digests "$dir/p1.out" | wc -l | tr -d ' ')"
((count > 0)) || ok=0
echo "  restart: the host resumed the kept town; $count digests compared: $([[ $ok == 1 ]] && echo identical || echo DIFFERENT)"
[[ $ok == 1 ]] || failed=1

if [[ $failed == 0 ]]; then
  echo "server identical: $(basename "$bin")"
  rm -rf "$out"
else
  echo "SERVER DIFFERENT: see $out"
fi
exit $failed
