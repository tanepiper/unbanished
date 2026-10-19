#!/usr/bin/env bash
# Run the compatibility scenarios with one game binary and keep what each produces.
#
#   tools/compat/scenarios.sh BINARY OUT [SCENARIO...]
#
# Each scenario starts the game headless with a fixed seed and scratch settings and save folder
# (the player's own are never touched), plays for a fixed time, pauses, saves and captures a
# frame: OUT/<scenario>.sav, OUT/<scenario>.png and OUT/<scenario>.log. Two runs of the same
# binary must produce identical saves and frames; two versions that keep the game's behaviour
# must too (tools/compat/compare.sh).
#
# Scenarios (default: all):
#   new-game      a new game (seed 1234, Medium), 25 s at speed 5
#   small-town    save-game/Save0.sav (29 citizens), 30 s at speed 1
#   medium-town   save-game/Save83.sav (116 citizens), 30 s at speed 5
#   large-town    save-game/Auto-1.sav (1,024 citizens), 40 s at speed 10
#   continue:PATH a save made by another version (PATH), 25 s at speed 5 (backward compatibility)
#
# Needs game-data/ (tools/extract-assets.sh) and the saves in save-game/ (local, git-ignored).
# Builds before 0.2.0 lack the `speed` script command (they run at speed 1) and may save on a
# GPU-dependent frame; compare them only with themselves.
set -euo pipefail
repo="$(cd "$(dirname "$0")/../.." && pwd)"
bin="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
out="$2"
shift 2
mkdir -p "$out"
out="$(cd "$out" && pwd)"
scenarios=("$@")
[[ ${#scenarios[@]} -eq 0 ]] && scenarios=(new-game small-town medium-town large-town)

run() { # name seconds speed args...
  local name="$1" secs="$2" speed="$3"
  shift 3
  local dir="$out/.$name"
  rm -rf "$dir" && mkdir -p "$dir/user"
  # The data folder is passed explicitly: a binary built by build.sh would otherwise look in the
  # checkout it was built in, which is gone.
  (
    cd "$repo"
    BANISHED_SEED=1234 BANISHED_USER_DIR="$dir/user" BANISHED_SETTINGS="$dir/settings.txt" \
      "$bin" --boot-capture "$out/$name.png" "$((secs + 2))" --capture-size 1280x800 \
      "$@" --script "speed $speed@10" --script "toggle_pause@$secs" \
      --script "save $out/$name.sav@$((secs + 1))" "$repo/game-data/original/WinData" \
      >"$out/$name.log" 2>&1 || true
  )
  rm -rf "$dir"
  if [[ -f "$out/$name.sav" ]]; then
    echo "  $name: saved"
  else
    echo "  $name: NO SAVE (see $out/$name.log)"
  fi
}

for s in "${scenarios[@]}"; do
  case "$s" in
    new-game) run new-game 25 5 --press buttonNew@4.5 --press buttonOk@5.0 ;;
    small-town) run small-town 30 1 --script "load save-game/Save0.sav@4.5" ;;
    medium-town) run medium-town 30 5 --script "load save-game/Save83.sav@4.5" ;;
    large-town) run large-town 40 10 --script "load save-game/Auto-1.sav@4.5" ;;
    continue:*)
      path="$(cd "$(dirname "${s#continue:}")" && pwd)/$(basename "${s#continue:}")"
      name="continue-$(basename "$path" .sav)"
      run "$name" 25 5 --script "load $path@4.5"
      ;;
    *) echo "unknown scenario: $s" >&2 && exit 2 ;;
  esac
done
