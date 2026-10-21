#!/usr/bin/env bash
# Run one of the Ghidra scripts in tools/ghidra on an already analysed project, without
# analysing again (about a minute of start-up, so batch addresses in one run).
#
#   tools/ghidra/run.sh BINARY JOB OUTPUT [ARGS ...]
#
#   tools/ghidra/run.sh exe decompile analysis/notes/ai.txt 0x1400be770 0x1400be680
#   tools/ghidra/run.sh dll named     analysis/notes/random.txt Random::Rand
#   tools/ghidra/run.sh exe refs      analysis/notes/game-refs.txt 0x1401cbdd8
#   tools/ghidra/run.sh exe vtable    analysis/notes/vt.txt Citizen 0x140165120 40
#
# BINARY: exe (Application-steam-x64.exe), dll (Runtime-steam-x64.dll), or a file name.
# JOB: decompile (ExportAddressBatch: also creates functions Ghidra missed, such as callbacks
# reached only through tables), named (ExportNamedFunctions: by name substring), refs
# (ExportAddressRefs: every reference to an address, with the referring function decompiled),
# vtable (ExportVtableSlots: LABEL 0xTABLE [SLOTS] ...), or a .java script in tools/ghidra.
# The project is the one already analysed for the binary (analysis/gameplay-project for the exe,
# analysis/runtime-identity-project for the DLL, else analysis/ghidra-project); --project DIR NAME
# picks another. Ghidra locks a project: one job per project at a time. Output stays in
# analysis/ (ignored by git). For a single function, tools/triage/decomp.py ADDR --headless is
# shorter.
set -euo pipefail
. "$(dirname "$0")/../re-env.sh"
project=""; name=""
if [[ "${1:-}" == --project ]]; then project="$2"; name="$3"; shift 3; fi
[[ $# -ge 3 ]] || { sed -n '2,24p' "$0"; exit 2; }
case "$1" in
  exe) binary=Application-steam-x64.exe ;;
  dll) binary=Runtime-steam-x64.dll ;;
  *) binary="$(basename "$1")" ;;
esac
job="$2"; output="$3"; shift 3
case "$job" in
  decompile) script=ExportAddressBatch.java ;;
  named) script=ExportNamedFunctions.java ;;
  refs) script=ExportAddressRefs.java ;;
  vtable) script=ExportVtableSlots.java ;;
  *.java) script="$job" ;;
  *) echo "unknown job $job (decompile, named, refs, vtable, or a .java script)" >&2; exit 2 ;;
esac
if [[ -z "$project" ]]; then
  case "$binary" in
    Application-steam-x64.exe) project=analysis/gameplay-project; name=banished-gameplay ;;
    Runtime-steam-x64.dll) project=analysis/runtime-identity-project; name=banished-runtime ;;
    *) project=analysis/ghidra-project; name=banished ;;
  esac
fi
ghidra="$(find_ghidra)"; java="$(find_java21)"
[[ -n "$ghidra" && -n "$java" ]] || { echo "Ghidra or JDK 21 not found: run tools/re-setup.sh" >&2; exit 1; }
[[ -f "$RE_ROOT/$project/$name.gpr" ]] || { echo "no Ghidra project $project/$name.gpr (tools/worktree-setup.sh in a worktree, or tools/ghidra/export.sh)" >&2; exit 1; }
mkdir -p "$(dirname "$output")"
output="$(cd "$(dirname "$output")" && pwd)/$(basename "$output")"
# The real path: Ghidra rejects a project path with a dot folder (a worktree in .claude/...).
project_dir="$(cd "$RE_ROOT/$project" && pwd -P)"
JAVA_HOME="$java" "$ghidra/support/analyzeHeadless" "$project_dir" "$name" -process "$binary" \
  -noanalysis -readOnly -scriptPath "$RE_ROOT/tools/ghidra" -postScript "$script" "$output" "$@" 2>&1 \
  | grep -E "written to|ERROR|Exception" || true
[[ -s "$output" ]] && echo "$output" || { echo "no output; see the messages above" >&2; exit 1; }
