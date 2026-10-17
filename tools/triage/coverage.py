#!/usr/bin/env python3
"""Which of the original's functions the port has not cited.

Every ported behaviour cites its original function (`FUN_1400be680`, `0x1800fd530`, or a
Runtime symbol name). This lists, per binary, the functions no tracked source or doc mentions,
and ranks the ones that matter: uncited functions called by cited ones (a step left out of a
path that is otherwise ported), then uncited functions reachable only from uncited ones.

Usage: python3 tools/triage/coverage.py [--binary Application|Runtime] [--min-size N] [--all]
       [--undefined]

`--undefined` also covers code Ghidra never made a function of: the targets of read-only data
pointers into the code section that start no known function (event dispatchers reached only
through vtables, such as StatusBarUI's at `0x14007f870`). Their sizes are shown, and every
call or jump inside them to a function no file cites (needs `objdump` and the binary in
game-data/original).

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
    ap.add_argument("--undefined", action="store_true", help="scan code outside Ghidra's functions")
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
    if args.undefined:
        undefined(binary, funcs, is_cited)


def undefined(binary, funcs, is_cited):
    """Pointers from the read-only data into code that is not a known function."""
    import bisect
    import struct
    import subprocess
    path = os.path.join(ROOT, "game-data", "original", binary)
    img = open(path, "rb").read()
    pe = struct.unpack_from("<I", img, 0x3C)[0]
    nsec = struct.unpack_from("<H", img, pe + 6)[0]
    opt = struct.unpack_from("<H", img, pe + 20)[0]
    base = struct.unpack_from("<Q", img, pe + 24 + 24)[0]
    secs = {}
    for k in range(nsec):
        o = pe + 24 + opt + 40 * k
        name = img[o:o + 8].rstrip(b"\0").decode()
        vsize, rva, rsize, raw = struct.unpack_from("<IIII", img, o + 8)
        secs[name] = (rva, max(vsize, rsize), raw)
    trva, tsize, traw = secs[".text"]
    starts = sorted(int(a, 16) for a in funcs)
    sizes = {int(a, 16): funcs[a][0] for a in funcs}
    rrva, rsize, rraw = secs[".rdata"]
    found = set()
    for i in range(rraw, rraw + rsize - 8, 8):
        v = struct.unpack_from("<Q", img, i)[0]
        if trva <= v - base < trva + tsize and v not in sizes:
            k = bisect.bisect_right(starts, v) - 1
            if not (k >= 0 and v < starts[k] + sizes[starts[k]]):
                found.add(v)
    bounds = sorted(set(starts) | found)
    print("# code outside Ghidra's functions, reached from data: address\tsize\tuncited targets")
    for v in sorted(found):
        j = bisect.bisect_right(bounds, v)
        end = bounds[j] if j < len(bounds) else v + 16
        o0, o1 = v - base - trva + traw, end - base - trva + traw
        n = len(img[o0:o1].rstrip(b"\xcc"))
        if n < 20:
            continue
        out = subprocess.run(
            ["objdump", "-d", "--no-show-raw-insn", f"--start-address={hex(v)}",
             f"--stop-address={hex(v + n)}", path], capture_output=True, text=True).stdout
        targets = sorted({m.group(1) for m in re.finditer(r"(?:call|jmp)q?\s+0x([0-9a-f]{9})", out)
                          if m.group(1) in funcs and not is_cited(m.group(1))})
        if targets:
            print(f"{v:x}\t{n}\t{','.join(targets)}")


if __name__ == "__main__":
    main()
