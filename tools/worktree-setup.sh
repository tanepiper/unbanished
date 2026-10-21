#!/usr/bin/env bash
# Make a git worktree ready to work in: link the main checkout's ignored folders into it.
#
#   tools/worktree-setup.sh          # in the worktree; prints what it linked
#
# A worktree (AI coding agents usually work in one) has none of the folders git ignores: the
# game data, the saves, the Ghidra analysis, the Python environment, the tool settings. This
# links each one that exists in the main checkout (game-data, save-game, analysis, .venv, .env)
# and adds the links to the repository's shared info/exclude, because a link is a file to git
# and the `/game-data/` rule in .gitignore does not match it. Safe to run again. In the main
# checkout it does nothing.
set -euo pipefail
here="$(git rev-parse --show-toplevel)"
main="$(cd "$(git rev-parse --git-common-dir)/.." && pwd)"
if [[ "$here" == "$main" ]]; then echo "This is the main checkout; nothing to link."; exit 0; fi
exclude="$(git rev-parse --git-common-dir)/info/exclude"
mkdir -p "$(dirname "$exclude")"; touch "$exclude"
for d in game-data save-game analysis .venv .env; do
  if [[ ! -e "$main/$d" ]]; then echo "  -      $d (not in the main checkout)"; continue; fi
  if [[ -L "$here/$d" ]]; then echo "  ok     $d"; 
  elif [[ -e "$here/$d" ]]; then echo "  kept   $d (a real folder or file is already here)"; continue
  else ln -s "$main/$d" "$here/$d"; echo "  linked $d -> $main/$d"; fi
  grep -qx "/$d" "$exclude" || echo "/$d" >> "$exclude"
done
if git -C "$here" status --short | grep -qE '^\?\? (game-data|save-game|analysis|\.venv|\.env)$'; then
  echo "warning: a link shows in git status; check $exclude" >&2; exit 1
fi
