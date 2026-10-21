# Where the reverse-engineering tools are: Ghidra, JDK 21, the Python environment.
# Source it (`. tools/re-env.sh`); it reads .env at the repository root first (tools/re-setup.sh
# writes one), then looks in the usual install places. tools/re_env.py does the same for Python.
RE_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
[[ -f "$RE_ROOT/.env" ]] && set -a && . "$RE_ROOT/.env" && set +a

find_ghidra() {
  for d in "${GHIDRA_INSTALL_DIR:-}" /opt/homebrew/opt/ghidra/libexec /usr/local/opt/ghidra/libexec \
           /opt/ghidra /usr/share/ghidra "$HOME"/ghidra_*_PUBLIC; do
    [[ -n "$d" && -x "$d/support/analyzeHeadless" ]] && { echo "$d"; return; }
  done
}

find_java21() {
  for d in "${JAVA_HOME:-}" /opt/homebrew/opt/openjdk@21/libexec/openjdk.jdk/Contents/Home \
           /usr/lib/jvm/java-21-openjdk-amd64 /usr/lib/jvm/java-21-openjdk /usr/lib/jvm/temurin-21-jdk-amd64; do
    [[ -n "$d" && -x "$d/bin/java" ]] && { echo "$d"; return; }
  done
}

RE_PYTHON="${RE_PYTHON:-$RE_ROOT/.venv/bin/python}"
