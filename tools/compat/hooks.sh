#!/usr/bin/env bash
# Rule hooks from end to end (docs/develop/RULE-HOOKS.md) with a test plugin,
# tools/test/hooks/every-minute.wat, that draws from the game's random stream once a game minute:
#
#   1. --sim-digest on a save: the hook changes the game (other digests than without it), two
#      runs with it give the same digests, and a vanilla run (--vanilla) ignores it;
#   2. two players with the hook play through a server and stay in step;
#   3. a player without the hook is refused by the room, with the reason.
#
#   tools/compat/hooks.sh BINARY
#
# Exit 0 when all hold. Results stay in target/compat/hooks/<binary>-<hash> when they do not.
set -euo pipefail
repo="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo"
source tools/compat/run-name.sh
bin="${1:?usage: tools/compat/hooks.sh BINARY}"
out="target/compat/hooks/$(run_name "$bin")"
rm -rf "$out" && mkdir -p "$out/with/Hooks" "$out/without"
cargo run -q --offline -p unbanished-hooks --example wat2wasm \
  tools/test/hooks/every-minute.wat "$out/with/Hooks/every-minute.wasm"
cargo build -q --offline --release -p unbanished-server
data="$repo/game-data/original/WinData"
failed=0

# 1. The save, with and without the hook.
digests() { # user-dir [extra arguments]
  local dir="$1"
  shift
  UNBANISHED_USER_DIR="$dir" UNBANISHED_SETTINGS="$dir/settings.txt" \
    "$bin" --sim-digest save-game/Save83.sav 1800 600 "$@" "$data" 2>>"$dir/log.txt" |
    awk '{ print $NF }' | tr '\n' ' '
}
without="$(digests "$out/without")"
with1="$(digests "$out/with")"
with2="$(digests "$out/with")"
vanilla="$(digests "$out/with" --vanilla)"
ok=1
[[ "$with1" == "$with2" ]] || ok=0
[[ "$with1" != "$without" ]] || ok=0
[[ "$vanilla" == "$without" ]] || ok=0
grep -q "rule hooks: every-minute" "$out/with/log.txt" || ok=0
echo "  save: the hook changes the game, twice the same, ignored in a vanilla run: $([[ $ok == 1 ]] && echo identical || echo DIFFERENT)"
[[ $ok == 1 ]] || failed=1

# 2 and 3. Two players with the hook; a third without it.
RECORD_DIR="$(mkdir -p "$out/recordings" && cd "$out/recordings" && pwd)" \
  tools/compat/scenarios.sh "$bin" "$out/runs" actions >/dev/null
recording="$out/recordings/actions.rec"
port=$((8000 + RANDOM % 100))
target/release/unbanished-server "127.0.0.1:$port" >"$out/server.log" 2>&1 &
server_pid=$!
trap 'kill $server_pid 2>/dev/null || true' EXIT
sleep 1
player() { # user-dir number role
  UNBANISHED_USER_DIR="$1" UNBANISHED_SETTINGS="$1/settings$2.txt" \
    "$bin" --net-test "ws://127.0.0.1:$port" "$3" HOOKS save-game/Save83.sav "$recording" 2 6 120 \
    "$data" >"$out/p$2.out" 2>"$out/p$2.log" &
  pids+=($!)
}
pids=()
mkdir -p "$out/with1/Hooks" && cp "$out/with/Hooks/every-minute.wasm" "$out/with1/Hooks/"
player "$out/with" 0 host
sleep 1
player "$out/with1" 1 join
ok=1
for pid in "${pids[@]}"; do wait "$pid" || ok=0; done
dg() { awk '$1 == "update" { l[$2] = $0 } END { for (u in l) print l[u] }' "$1" | sort -n -k2; }
diff <(dg "$out/p0.out") <(dg "$out/p1.out") >/dev/null || ok=0
grep -q "resyncs \[\]" "$out/p0.out" || ok=0
count="$(dg "$out/p1.out" | wc -l | tr -d ' ')"
echo "  players: two with the hook, $count digests compared: $([[ $ok == 1 ]] && echo identical || echo DIFFERENT)"
[[ $ok == 1 ]] || failed=1
# The host opens the room again and waits; a player without the hook is refused.
pids=()
player "$out/with" 2 host
sleep 2
player "$out/without" 3 join
wait "${pids[1]}" && refused=0 || refused=1
kill "${pids[0]}" 2>/dev/null || true
ok=1
((refused == 1)) || ok=0
grep -q "rule hooks differ" "$out/p3.log" || ok=0
echo "  refused: a player without the hook is told \"$(grep -o "this game's rule hooks differ[^:]*" "$out/p3.log" | head -1)\": $([[ $ok == 1 ]] && echo identical || echo DIFFERENT)"
[[ $ok == 1 ]] || failed=1

if [[ $failed == 0 ]]; then
  echo "hooks identical: $(basename "$bin")"
  rm -rf "$out"
else
  echo "HOOKS DIFFERENT: see $out"
fi
exit $failed
