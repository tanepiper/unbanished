#!/usr/bin/env bash
# Run GameFrame's tools from this repository.
#
#   tools/gf.sh help                     GameFrame's own help (steps, skills, tools)
#   tools/gf.sh TOOL ARGS...             run a GameFrame tool, for example
#   tools/gf.sh tools/triage/hashnames.py game-data/extracted --aligned --names-from game-data/original/Application-steam-x64.exe
#
# GameFrame's skills and agents come from its Claude Code plugin (.claude/settings.json). Their
# commands name tools by GameFrame's paths (tools/triage/decomp_query.py ...): run those through
# this script. It finds GameFrame in $GAMEFRAME_ROOT, then next to this repository
# (../game-frame or ../reverse-engineer-games), then in ~/code. If none exists, clone it:
#   git clone https://github.com/tanepiper/game-frame ../game-frame
# The tools run in this folder, so their paths (game-data/, analysis/, labels/) are this
# project's. GameFrame's decomp_query.py reads labels/names.tsv from here.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
for d in "${GAMEFRAME_ROOT:-}" "$here/../game-frame" "$here/../reverse-engineer-games" \
         "$HOME/code/game-frame" "$HOME/code/reverse-engineer-games"; do
  if [[ -n "$d" && -f "$d/gameframe" && -d "$d/tools" ]]; then gf="$(cd "$d" && pwd)"; break; fi
done
if [[ -z "${gf:-}" ]]; then
  echo "GameFrame not found: set GAMEFRAME_ROOT, or git clone https://github.com/tanepiper/game-frame ../game-frame" >&2
  exit 1
fi
python="$gf/.venv/bin/python"
[[ -x "$python" ]] || python="python3"
if [[ $# -eq 0 || "$1" == "help" || "$1" == "-h" ]]; then
  exec "$python" "$gf/gameframe" "${@:-help}"
fi
tool="$1"; shift
[[ -f "$gf/$tool" ]] || { echo "no tool $tool in $gf" >&2; exit 1; }
case "$tool" in
  *.py) exec "$python" "$gf/$tool" "$@" ;;
  *) exec "$gf/$tool" "$@" ;;
esac
