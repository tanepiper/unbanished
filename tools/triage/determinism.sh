#!/usr/bin/env bash
# Run the same scripted boot capture twice with a fixed seed and compare the frames. A 1:1 port
# must be deterministic: any difference means unseeded randomness, iteration-order dependence
# (hash maps) or uninitialised state crept in.
#
# Usage: tools/triage/determinism.sh [SECONDS] [SEED]   (defaults 16, 1234)
# Exit 0 when identical; prints the imgdiff report and keeps the frames otherwise.
set -euo pipefail
cd "$(dirname "$0")/../.."
secs="${1:-16}"
seed="${2:-1234}"
out="$(mktemp -d "${TMPDIR:-/tmp}/determinism.XXXXXX")"
cargo build -q --release -p banished-app --bin banished --example imgdiff
for run in a b; do
  BANISHED_SEED="$seed" ./target/release/banished --boot-capture "$out/$run.png" "$secs" \
    --press buttonNew@4.5 --press buttonOk@5.0 --wheel 8@9 --wheel 8@9.3 --wheel 6@9.6 \
    >/dev/null 2>&1
done
report="$(./target/release/examples/imgdiff "$out/a.png" "$out/b.png" --out "$out/diff.png" --json)"
if grep -q '"mismatch_pct":0.000' <<<"$report" && grep -q '"max_delta":0' <<<"$report"; then
  echo "deterministic: identical frames at ${secs}s (seed $seed)"
  rm -rf "$out"
else
  echo "NOT deterministic at ${secs}s (seed $seed): $report"
  echo "frames and heat map kept in $out"
  exit 1
fi
