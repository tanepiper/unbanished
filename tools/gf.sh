#!/usr/bin/env bash
# Run GameFrame's tools from this repository.
#
#   tools/gf.sh help                     GameFrame's own help (steps, skills, tools)
#   tools/gf.sh TOOL ARGS...             run a GameFrame tool, for example
#   tools/gf.sh tools/triage/hashnames.py game-data/extracted --aligned --names-from game-data/original/Application-steam-x64.exe
#
# GameFrame's skills and agents come from its Claude Code plugin (.claude/settings.json). Their
# commands name tools by GameFrame's paths (tools/triage/decomp_query.py ...): run those through
# this script. It uses the machine's one GameFrame checkout: $GAMEFRAME_ROOT, from the
# environment or this project's .env, else ~/code/game-frame or
# ~/code/reverse-engineer-games. It never looks next to this repository: a second clone there
# would fall behind. To change GameFrame, use a worktree of that checkout (AGENTS.md).
# The tools run in this folder with GAMEFRAME_PROJECT set to it, so their paths (game-data/,
# analysis/, labels/) are this project's. GameFrame's decomp_query.py reads labels/names.tsv
# from here.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
if [[ -z "${GAMEFRAME_ROOT:-}" && -f "$here/.env" ]]; then
  GAMEFRAME_ROOT="$(sed -n 's/^GAMEFRAME_ROOT=//p' "$here/.env" | tail -1)"
  GAMEFRAME_ROOT="${GAMEFRAME_ROOT/#\~/$HOME}"
fi
for d in "${GAMEFRAME_ROOT:-}" "$HOME/code/game-frame" "$HOME/code/reverse-engineer-games"; do
  if [[ -n "$d" && -f "$d/gameframe" && -d "$d/tools" ]]; then gf="$(cd "$d" && pwd)"; break; fi
done
if [[ -z "${gf:-}" ]]; then
  echo "GameFrame not found: set GAMEFRAME_ROOT in .env to the machine's one GameFrame checkout" >&2
  exit 1
fi
export GAMEFRAME_PROJECT="$here"
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
