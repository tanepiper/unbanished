#!/usr/bin/env bash
# Build the browser version of the game into site/public/game/, for the Play page.
#
#   site/scripts/build-game.sh [BASE]     # BASE is the site's base path, default /unbanished/
#
# Needs: rustup target add wasm32-unknown-unknown, and trunk (cargo install trunk).
set -euo pipefail

site="$(cd "$(dirname "$0")/.." && pwd)"
base="${1:-${SITE_BASE:-/unbanished/}}"
cd "$site/../apps/game"
trunk build --release --public-url "${base}game/" --dist "$site/public/game"
echo "game: $site/public/game (served at ${base}game/)"
