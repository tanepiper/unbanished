#!/usr/bin/env python3
"""Which of the original's functions the port has not cited.

Every ported behaviour cites its original function (`FUN_1400be680`, `0x1800fd530`, or a
Runtime symbol name). This lists, per binary, the functions no tracked source or doc mentions,
and ranks the ones that matter: uncited functions called by cited ones (a step left out of a
path that is otherwise ported), then uncited functions reachable only from uncited ones.

Usage: python3 tools/triage/coverage.py [--binary Application|Runtime] [--min-size N] [--all]

Reads the local Ghidra export (analysis/export/<binary>/functions.tsv and decomp.c, git-ignored)
and prints addresses, sizes and counts only.
"""

import argparse
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
BINARIES = {
    "Application": "Application-steam-x64.exe",
    "Runtime": "Runtime-steam-x64.dll",
}
SOURCE_DIRS = ["apps", "crates", "docs", "tools"]
SKIP_DIRS = {"target", "node_modules", ".git", "dist"}


def cited():
    """Every hex address (8+ digits) and `Class::Method` name mentioned in tracked files."""
    addresses, names = set(), set()
    hex_re = re.compile(r"(?:FUN_|0x)?(1[48]0[0-9a-fA-F]{6})\b")
    name_re = re.compile(r"\b([A-Z][A-Za-z0-9_]*::[A-Za-z_~][A-Za-z0-9_]*)")
    for top in SOURCE_DIRS:
        for base, dirs, files in os.walk(os.path.join(ROOT, top)):
            dirs[:] = [d for d in dirs if d not in SKIP_DIRS]
            for f in files:
                if not f.endswith((".rs", ".md", ".py", ".wgsl", ".tsv", ".txt")):
                    continue
                try:
                    text = open(os.path.join(base, f), encoding="utf-8", errors="ignore").read()
                except OSError:
                    continue
                addresses.update(m.lower() for m in hex_re.findall(text))
                names.update(name_re.findall(text))
    return addresses, names


def functions(binary):
    path = os.path.join(ROOT, "analysis", "export", binary, "functions.tsv")
    out = {}
    for line in open(path, encoding="utf-8", errors="ignore"):
        parts = line.rstrip("\n").split("\t")
        if len(parts) < 3:
            continue
        addr, size, name = parts[0].lower(), int(parts[1] or 0), parts[2]
        kind = parts[3] if len(parts) > 3 else ""
        out[addr] = (size, name, kind)
    return out


def calls(binary):
    """Callee addresses per function, from the decompiled bodies."""
    path = os.path.join(ROOT, "analysis", "export", binary, "decomp.c")
    graph, current = {}, None
    head = re.compile(r"^// ==== ([0-9a-f]+) ")
    call = re.compile(r"FUN_([0-9a-f]{9})\s*\(")
    for line in open(path, encoding="utf-8", errors="ignore"):
        m = head.match(line)
        if m:
            current = m.group(1)
            graph[current] = set()
            continue
        if current:
            graph[current].update(call.findall(line))
    return graph


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--binary", choices=BINARIES, default="Application")
    ap.add_argument("--min-size", type=int, default=120, help="skip smaller functions")
    ap.add_argument("--all", action="store_true", help="also list uncited, uncalled ones")
    args = ap.parse_args()
    binary = BINARIES[args.binary]
    addresses, names = cited()
    funcs = functions(binary)
    graph = calls(binary)

    def is_cited(addr):
        size, name, _ = funcs[addr]
        if addr in addresses:
            return True
        short = name.split("::")
        return len(short) >= 2 and "::".join(short[-2:]) in names

    def is_library(name, kind):
        return kind == "thunk" or "::" in name and not name.startswith("FUN_") and (
            name.split("::")[0] in ("std", "Concurrency") or "RUNTIME" in name
        )

    covered = {a for a in funcs if is_cited(a)}
    print(f"{binary}: {len(funcs)} functions, {len(covered)} cited", file=sys.stderr)
    missed = {}
    for caller in covered:
        for callee in graph.get(caller, ()):
            if callee in funcs and callee not in covered:
                size, name, kind = funcs[callee]
                if size >= args.min_size and not is_library(name, kind):
                    missed.setdefault(callee, set()).add(caller)
    rows = sorted(missed.items(), key=lambda kv: (-funcs[kv[0]][0]))
    print("# uncited, called from cited functions: address\tsize\tname\tcallers")
    for addr, callers in rows:
        size, name, _ = funcs[addr]
        print(f"{addr}\t{size}\t{name}\t{','.join(sorted(callers)[:4])}")
    if args.all:
        rest = [
            a for a in funcs
            if a not in covered and a not in missed and funcs[a][0] >= args.min_size
            and not is_library(funcs[a][1], funcs[a][2])
        ]
        print("# uncited, not called from cited functions")
        for a in sorted(rest, key=lambda a: -funcs[a][0]):
            size, name, _ = funcs[a]
            print(f"{a}\t{size}\t{name}")


if __name__ == "__main__":
    main()
