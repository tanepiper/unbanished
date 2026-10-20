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
#   actions       save-game/Save83.sav, 30 s at speed 5, with the player's actions through the
#                 interface: worker buttons, placing a house (turned), a road drag, clearing
#                 trees, demolishing by drag, the Path tool, a random town name and a rename,
#                 pause and play. Positions are for Save83's saved view at 1280x800. It needs a
#                 build with `--mouse-down` / `--mouse-up` (0.3.0 and later).
#   continue:PATH a save made by another version (PATH), 25 s at speed 5 (backward compatibility)
#
# RECORD_DIR=DIR also records each scenario that loads a save (`--record DIR/<scenario>.rec
# --record-every 60`), for tools/compat/replay.sh. With GNU timeout installed (`gtimeout`,
# Homebrew's coreutils), a run that takes more than 15 minutes is stopped.
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
[[ ${#scenarios[@]} -eq 0 ]] && scenarios=(new-game small-town medium-town large-town actions)

limit=()
if command -v gtimeout >/dev/null; then limit=(gtimeout 900); fi

run() { # name seconds speed args...
  local name="$1" secs="$2" speed="$3"
  shift 3
  local record=()
  [[ -n ${RECORD_DIR:-} ]] && record=(--record "$RECORD_DIR/$name.rec" --record-every 60)
  local dir="$out/.$name"
  rm -rf "$dir" && mkdir -p "$dir/user"
  # The data folder is passed explicitly: a binary built by build.sh would otherwise look in the
  # checkout it was built in, which is gone.
  (
    cd "$repo"
    # Builds before the rename to unbanished read BANISHED_*, later ones UNBANISHED_*: set both,
    # so an old build never falls back to the player's own settings and saves.
    UNBANISHED_SEED=1234 BANISHED_SEED=1234 UNBANISHED_USER_DIR="$dir/user" BANISHED_USER_DIR="$dir/user" UNBANISHED_SETTINGS="$dir/settings.txt" BANISHED_SETTINGS="$dir/settings.txt" \
      ${limit[@]+"${limit[@]}"} "$bin" --boot-capture "$out/$name.png" "$((secs + 2))" \
      --capture-size 1280x800 ${record[@]+"${record[@]}"} "$@" --script "speed $speed@10" --script "toggle_pause@$secs" \
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
    actions)
      run actions 30 5 --script "load save-game/Save83.sav@4.5" \
        --click-at 845,390@10.5 --press workerCountUp@11.0 --press jobCountUp@11.3 \
        --press workerCountDown@11.6 --press buttonClose@12.0 \
        --script "tool Template/WoodHouse.rsc:resource@12.5" --hold StructureRight@12.6-12.7 \
        --move 150,160@12.8 --click-at 150,160@13.0 \
        --script "tool Template/DirtRoad.rsc:resource@13.5" --mouse-down 945,470@13.8 \
        --move 980,470@14.0 --move 1010,470@14.1 --mouse-up 1010,470@14.4 \
        --script "tool Template/ClearTree.rsc:resource@14.8" --mouse-down 300,150@15.0 \
        --move 380,200@15.2 --mouse-up 420,230@15.4 \
        --script "tool Template/ClearBuildings.rsc:resource@15.8" --mouse-down 280,430@16.0 \
        --move 400,480@16.2 --mouse-up 470,530@16.4 \
        --script "tool Template/UtilityPath.rsc@16.8" --click-at 760,610@17.1 --hold Cancel@17.5-17.6 \
        --script "pause@18" --press buttonMap@18.4 --press buttonNewName@18.8 \
        --press buttonNewName@19.1 --press buttonOk@19.4 --press buttonResume@19.8 \
        --hold TogglePause@20-20.1 --hold TogglePause@20.5-20.6
      ;;
    continue:*)
      path="$(cd "$(dirname "${s#continue:}")" && pwd)/$(basename "${s#continue:}")"
      name="continue-$(basename "$path" .sav)"
      run "$name" 25 5 --script "load $path@4.5"
      ;;
    *) echo "unknown scenario: $s" >&2 && exit 2 ;;
  esac
done
