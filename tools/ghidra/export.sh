#!/usr/bin/env bash
# Run Ghidra headless analysis over the original binaries and export decompiled C,
# function lists, strings, imports and exports into analysis/ (git-ignored).
# Requires tools/extract-assets.sh to have been run first.
#
#   GHIDRA_INSTALL_DIR=/opt/homebrew/opt/ghidra/libexec tools/ghidra/export.sh
set -euo pipefail

repo="$(cd "$(dirname "$0")/../.." && pwd)"
ghidra="${GHIDRA_INSTALL_DIR:-/opt/homebrew/opt/ghidra/libexec}"
bins="$repo/game-data/original"
out="$repo/analysis"

if [[ ! -x "$ghidra/support/analyzeHeadless" ]]; then
  echo "Ghidra not found at $ghidra; set GHIDRA_INSTALL_DIR." >&2; exit 1
fi
if [[ ! -f "$bins/Runtime-steam-x64.dll" ]]; then
  echo "Original binaries missing; run tools/extract-assets.sh first." >&2; exit 1
fi

mkdir -p "$out/ghidra-project" "$out/export"
MAXMEM="${MAXMEM:-8G}" "$ghidra/support/analyzeHeadless" "$out/ghidra-project" banished \
  -overwrite -max-cpu "${GHIDRA_CPUS:-6}" \
  -import "$bins/Runtime-steam-x64.dll" "$bins/Application-steam-x64.exe" \
          "$bins/VideoDX11-steam-x64.dll" "$bins/VideoDX9-steam-x64.dll" \
  -scriptPath "$repo/tools/ghidra" -postScript ExportAll.java "$out/export" \
  2>&1 | tee "$out/headless.log" | grep -E --line-buffered "EXPORT DONE|Analysis succeeded|ERROR.*Exception"
