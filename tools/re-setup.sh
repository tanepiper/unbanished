#!/usr/bin/env bash
# Check, and optionally install, the reverse-engineering toolchain: what tracing, emulation and
# the triage tools need (the game itself only needs Rust; see docs/develop/BUILDING.md).
#
#   tools/re-setup.sh            check only: a table of what is present and what is missing
#   tools/re-setup.sh --install  install what is missing (Homebrew on macOS, apt on Debian/Ubuntu)
#                                and create .venv with unicorn and pefile
#   tools/re-setup.sh --rea      also show REA's registration plan for your agent (dry run)
#
# Sizes: Ghidra ~500 MB, JDK 21 ~300 MB, the Python packages ~40 MB. On Linux, Ghidra is not in
# apt: the script names the download page. Writes .env (GHIDRA_INSTALL_DIR, JAVA_HOME), which
# tools/re-env.sh, tools/re_env.py and .mcp.json read. In a git worktree, run
# tools/worktree-setup.sh instead: it links the main checkout's .venv and .env.
set -uo pipefail
. "$(dirname "$0")/re-env.sh"
install=0; rea=0
for a in "$@"; do
  case "$a" in
    --install) install=1 ;;
    --rea) rea=1 ;;
    -h|--help) sed -n '2,15p' "$0"; exit 0 ;;
    *) echo "unknown option $a" >&2; exit 2 ;;
  esac
done
os="$(uname -s)"; missing=0
row() { printf '  %-12s %-8s %s\n' "$1" "$2" "$3"; }
have() { command -v "$1" >/dev/null 2>&1; }
pkg() { # brew-name apt-name
  [[ $install == 1 ]] || return 1
  if [[ $os == Darwin ]] && have brew; then brew install "$1" >/dev/null
  elif have apt-get && [[ -n "${2:-}" ]]; then sudo apt-get install -y "$2" >/dev/null
  else return 1; fi
}
check() { # label test brew apt hint
  if eval "$2" >/dev/null 2>&1 || { pkg "$3" "$4" && eval "$2" >/dev/null 2>&1; }; then
    row "$1" ok "$(eval "$2" 2>/dev/null | head -1)"
  else row "$1" MISSING "$5"; missing=$((missing + 1)); fi
}

echo "Reverse-engineering toolchain ($os):"
check python3 "python3 --version" python@3.12 python3-venv "Python 3.10 or newer"
check java21 "find_java21" openjdk@21 openjdk-21-jdk "JDK 21 for Ghidra; or set JAVA_HOME in .env"
[[ -z "$(find_ghidra)" && $install == 1 && $os == Darwin ]] && have brew && brew install ghidra >/dev/null
if [[ -n "$(find_ghidra)" ]]; then row ghidra ok "$(find_ghidra)"
else row ghidra MISSING "https://github.com/NationalSecurityAgency/ghidra/releases; set GHIDRA_INSTALL_DIR in .env"; missing=$((missing + 1)); fi
check objdump "objdump --version" binutils binutils "binutils or LLVM objdump"
check node "node --version" node nodejs "Node.js 22.23.4+ or 24.22.1+ (REA runs with npx)"
if [[ ! -x "$RE_ROOT/.venv/bin/python" && $install == 1 ]]; then
  python3 -m venv "$RE_ROOT/.venv" && "$RE_ROOT/.venv/bin/pip" install -q unicorn pefile
fi
if "$RE_PYTHON" -c "import unicorn, pefile" 2>/dev/null; then
  row .venv ok "unicorn $("$RE_PYTHON" -c 'import unicorn; print(unicorn.__version__)'), pefile"
else row .venv MISSING "tools/re-setup.sh --install (python3 -m venv .venv; .venv/bin/pip install unicorn pefile)"; missing=$((missing + 1)); fi
[[ -f "$RE_ROOT/game-data/original/Application-steam-x64.exe" ]] && row game-data ok "game-data/original" \
  || row game-data -- "tools/extract-assets.sh (or tools/worktree-setup.sh in a worktree)"
[[ -f "$RE_ROOT/analysis/export/Application-steam-x64.exe/decomp.c" ]] && row analysis ok "analysis/export" \
  || row analysis -- "tools/ghidra/export.sh (~20 min; or tools/worktree-setup.sh in a worktree)"
if [[ -n "$(find_ghidra)" && -n "$(find_java21)" && ! -e "$RE_ROOT/.env" ]]; then
  printf 'GHIDRA_INSTALL_DIR=%s\nJAVA_HOME=%s\n' "$(find_ghidra)" "$(find_java21)" > "$RE_ROOT/.env"
  echo "  wrote .env (GHIDRA_INSTALL_DIR, JAVA_HOME)"
fi
if [[ $rea == 1 ]]; then
  echo; echo "REA registration plan (dry run; nothing changes):"
  npx -y rea-agents@"${REA_MCP_VERSION:-6.7.0}" setup --client "${REA_CLIENT:-claude_code}" --dry-run || echo "  could not run REA's setup"
fi
echo
if [[ $missing == 0 ]]; then echo "The toolchain is in place."; else echo "$missing missing: run tools/re-setup.sh --install"; exit 1; fi
