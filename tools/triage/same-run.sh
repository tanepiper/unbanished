#!/usr/bin/env bash
# Prove that two builds simulate a loaded save identically: both load SAVE, run at SPEED for
# SECONDS of capture time, save the game and capture a frame. The saves' game state and the
# frames must match exactly. Use it to check an optimisation against the build before it.
#
#   tools/triage/same-run.sh BASE_BINARY NEW_BINARY SAVE [SECONDS] [SPEED]   (defaults 40, 10)
#
# The saves are compared with the `savegame_same` example (game state, ignoring the header's
# date), the frames with `imgdiff`. Exit 0 when both match.
set -euo pipefail
cd "$(dirname "$0")/../.."
base="$1"
new="$2"
save="$3"
secs="${4:-40}"
speed="${5:-10}"
out="$(mktemp -d "${TMPDIR:-/tmp}/same-run.XXXXXX")"
cargo build -q --release -p banished-app --example imgdiff
cargo build -q --release -p banished-formats --example savegame_same
for run in base new; do
  bin="$base"
  [[ "$run" == new ]] && bin="$new"
  mkdir -p "$out/$run/user"
  BANISHED_USER_DIR="$out/$run/user" BANISHED_SETTINGS="$out/$run/settings.txt" BANISHED_SEED=1234 \
    "$bin" --boot-capture "$out/$run.png" "$((secs + 2))" --capture-size 1280x800 \
    --script "load $save@4.5" --script "speed $speed@10" \
    --script "toggle_pause@$secs" --script "save $out/$run.sav@$((secs + 1))" \
    >"$out/$run.log" 2>&1
done
status=0
if ! ./target/release/examples/savegame_same "$out/base.sav" "$out/new.sav" >"$out/save-diff.txt" 2>&1; then
  status=1
fi
report="$(./target/release/examples/imgdiff "$out/base.png" "$out/new.png" --json)"
if ! grep -q '"mismatch_pct":0.000' <<<"$report" || ! grep -q '"max_delta":0' <<<"$report"; then
  status=1
fi
echo "frames: $report"
echo "saves: $(head -c 400 "$out/save-diff.txt")"
if [[ $status == 0 ]]; then
  echo "identical: same saves and frames after ${secs}s at speed $speed"
  rm -rf "$out"
else
  echo "DIFFERENT; files kept in $out"
fi
exit $status
