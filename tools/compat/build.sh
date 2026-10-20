#!/usr/bin/env bash
# Build any version of the game in a clean checkout, for comparing versions.
#
#   tools/compat/build.sh REV [--keep]      # REV: a tag, branch or commit, e.g. v0.1.0, HEAD~3
#
# Checks REV out as a detached git worktree in target/compat/src/<sha> (nothing from this working
# folder leaks in: no uncommitted changes, no local files), builds the release game there, and
# copies the binary to target/compat/bin/banished-<sha>. Prints that path. A binary that already
# exists is reused. The worktree is removed afterwards unless --keep is given.
#
# All revisions share one cargo target folder (target/compat/cargo), so dependencies are built
# once. The build id is the commit (UNBANISHED_BUILD), so `unbanished --version` names it.
set -euo pipefail
repo="$(cd "$(dirname "$0")/../.." && pwd)"
rev="${1:?usage: tools/compat/build.sh REV [--keep]}"
keep="${2:-}"
sha="$(git -C "$repo" rev-parse --short=7 "$rev^{commit}")"
root="$repo/target/compat"
bin="$root/bin/banished-$sha"
if [[ -x "$bin" ]]; then
  echo "$bin"
  exit 0
fi
mkdir -p "$root/bin" "$root/src"
src="$root/src/$sha"
if [[ ! -d "$src" ]]; then
  git -C "$repo" worktree add --detach --quiet "$src" "$sha"
fi
# Revisions before the rename to `unbanished` (docs/ARCHITECTURE-PLAN.md, phase R) call the
# package banished-app and the program banished: read both names from the revision itself.
package="$(sed -n 's/^name = "\(.*\)"/\1/p' "$src/apps/game/Cargo.toml" | head -1)"
program="$(sed -n '/^\[\[bin\]\]/,/^name/s/^name = "\(.*\)"/\1/p' "$src/apps/game/Cargo.toml" | head -1)"
(
  cd "$src"
  UNBANISHED_BUILD="$sha" CARGO_TARGET_DIR="$root/cargo" \
    cargo build --release --offline -p "$package" --bin "$program" >&2
)
cp "$root/cargo/release/$program" "$bin"
if [[ "$keep" != "--keep" ]]; then
  git -C "$repo" worktree remove --force "$src"
fi
echo "$bin"
