#!/usr/bin/env python3
"""Name the library code in the original binaries: the statically linked MSVC C runtime (with its
few C++ standard library classes), the Steam stub, and the exception funclets Ghidra left
unnamed. Writes `labels/library.tsv`, which the decompile tools read after `labels/names.tsv`
and `labels/names-from-port.tsv`, and which `fnindex.py coverage` uses to keep library code out
of the game's figures.

Usage: python3 tools/triage/library.py [--out labels/library.tsv] [--check FILE] [--report]

The evidence, strongest first (the `evidence` column):

- `fid`: Ghidra's Function ID analyser matched the function's bytes against its Visual Studio
  2012 to 2019 x64 databases during the export (`tools/ghidra/export.sh` runs the default
  analysers), and the function lies in a cluster of such matches. `fid-conflict`: the same, but
  several library functions share the bytes (Ghidra's `FID_conflict:` names), so the label is
  one of them.
- `link-order`: Function ID found no match (functions too short, or not in its databases), but
  the function sits in a run of at most MAX_RUN unnamed functions between library functions.
  The linker keeps each object file's code together, so such a run belongs to the same library
  objects. It must also call only library code, or functions that the matched library code
  itself calls (the program's own `operator new` and `operator delete`).
- `start-code`: checked by hand (START below).
- `steam-stub`: the code in the `.bind` section, which holds the image's entry point. Steam's
  DRM wrapper adds it; it decrypts the game and jumps to the C runtime's entry.
- `handler` (kind `handler`): unnamed exception funclets (`__except` filters and `__finally`
  blocks), placed with the `Unwind@` funclets Ghidra did name.

A Function ID match outside a cluster (fewer than MIN_CLUSTER matches between long runs of the
binary's own functions) is a false match on a short generic body, such as the Runtime's
scalar deleting destructors that Function ID calls `CMFCShadowWnd::...`. Those stay game code.

Reads the local Ghidra export (analysis/export/<binary>/functions.tsv, decomp.c, exports.txt)
and prints addresses, sizes and names only. Standard library only.
"""
import argparse
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from ghidra_index import ROOT, Index, is_handler_name, is_thunk_name  # noqa: E402

BINARIES = ['Application-steam-x64.exe', 'Runtime-steam-x64.dll']
OUT = os.path.join(ROOT, 'labels', 'library.tsv')
MAX_RUN = 16
MIN_CLUSTER = 3
# CRT start code, checked by hand. It calls the program (WinMain, DllMain), so the callee check
# cannot pass it. In the Application, the first calls the security cookie set-up and then the
# second, which reads the startup info, sets up the heap, threads, I/O and environment, runs
# `_cinit`, calls `_wincmdln`, passes the image base and command line to WinMain (0x140042810),
# then exits. In the Runtime, the image entry point calls the security cookie set-up and then
# the second, which calls the third (`_cinit`, `_setargv`, `_mtinit`, `_ioterm` and the other
# start and stop steps) around DllMain (0x1800a0ac0).
START = {
    '14003bb20': 'WinMainCRTStartup',
    '14003bb34': '__tmainCRTStartup',
    '180080eac': '_DllMainCRTStartup',
    '180080eec': '__DllMainCRTStartup',
    '180080d4c': '_CRT_INIT',
}
# The program's own functions that library code may call, checked by hand. The Runtime replaces
# `operator delete` with one that frees through `System::MemoryAllocator::Free` under the
# allocator's lock; the C++ exception classes' deleting destructors call it.
CALLED_BY_LIBRARY = {
    '180006a10': 'operator delete',
}
HEADER = """\
# Library code in the original binaries, made by tools/triage/library.py from the Ghidra export:
# address, label, kind (library or handler), evidence (see the tool). Do not edit by hand.
# The decompile tools read it after names.tsv and names-from-port.tsv; fnindex.py coverage
# leaves these functions out of the game's code.
"""


def exports(binary):
    path = os.path.join(ROOT, 'analysis', 'export', binary, 'exports.txt')
    out = set()
    if os.path.exists(path):
        for line in open(path, errors='replace'):
            cols = line.split('\t')
            if len(cols) >= 2:
                out.add(cols[0].lower())
    return out


def bind_section(ix):
    """(start, end) of the `.bind` section, or None."""
    if ix.image is None:
        return None
    d = ix.image.data
    import struct
    pe = struct.unpack_from('<I', d, 0x3C)[0]
    nsec = struct.unpack_from('<H', d, pe + 6)[0]
    opt = struct.unpack_from('<H', d, pe + 20)[0]
    for i in range(nsec):
        s = pe + 24 + opt + 40 * i
        if d[s:s + 8].rstrip(b'\0') == b'.bind':
            vsize, va = struct.unpack_from('<II', d, s + 8)
            return ix.image.base + va, ix.image.base + va + vsize
    return None


def classify(binary):
    """{address: (label, kind, evidence)} for the library code and unnamed funclets."""
    ix = Index(binary)
    exported = exports(binary)
    addrs = sorted(ix.size, key=lambda a: int(a, 16))

    def kind(a):
        n = ix.name[a]
        if is_handler_name(n):
            return 'handler'
        if is_thunk_name(n):
            return 'thunk'
        if n.startswith('FUN_'):
            return 'fun'
        if a in START:
            return 'fid'
        if a in exported or n == 'entry':
            return 'own'
        return 'fid'

    k = {a: kind(a) for a in addrs}
    out = {}

    # Clusters: the functions between two long runs of the binary's own code.
    clusters, current = [], []
    i = 0
    while i < len(addrs):
        a = addrs[i]
        if k[a] == 'fun':
            j = i
            while j < len(addrs) and k[addrs[j]] == 'fun':
                j += 1
            if j - i > MAX_RUN:
                clusters.append(current)
                current = []
            else:
                current.extend(addrs[i:j])
            i = j
            continue
        if k[a] == 'own':
            clusters.append(current)
            current = []
        else:
            current.append(a)
        i += 1
    clusters.append(current)
    lib = set()
    for c in clusters:
        if sum(1 for a in c if k[a] == 'fid') >= MIN_CLUSTER:
            lib.update(a for a in c if k[a] == 'fid')

    # Unnamed runs between library functions (handlers and thunks may border them).
    cands = set()
    i = 0
    while i < len(addrs):
        if k[addrs[i]] != 'fun':
            i += 1
            continue
        j = i
        while j < len(addrs) and k[addrs[j]] == 'fun':
            j += 1
        left = addrs[i - 1] if i > 0 else None
        right = addrs[j] if j < len(addrs) else None
        sides = [left in lib, right in lib]
        border = [k.get(left) in ('handler', 'thunk'), k.get(right) in ('handler', 'thunk')]
        if j - i <= MAX_RUN and any(sides) and all(s or b for s, b in zip(sides, border)):
            cands.update(addrs[i:j])
        elif k.get(left) == 'handler' and k.get(right) == 'handler':
            # Funclets between named funclets, and the CRT's `__except` filters placed just
            # before the `Unwind@` block (their left neighbour is a named `$filt$`/`$fin$`).
            for a in addrs[i:j]:
                out[a] = (f'funclet_{a}', 'handler', 'handler')
        i = j

    # The functions the matched library code calls (operator new and delete, the program's own).
    shared = {c for a in lib for c in ix.callees.get(a, ())} | set(CALLED_BY_LIBRARY)
    keep = set(cands)
    while True:
        ok = lib | keep | shared
        nxt = {a for a in keep
               if all(c in ok or k.get(c) == 'thunk' for c in ix.callees.get(a, ()))}
        if nxt == keep:
            break
        keep = nxt

    for a in lib:
        n = ix.name[a]
        if n.startswith('FID_conflict:'):
            out[a] = (n.removeprefix('FID_conflict:'), 'library', 'fid-conflict')
        else:
            out[a] = (n, 'library', 'fid')
    for a in keep:
        out[a] = (f'crt_{a}', 'library', 'link-order')
    for a, name in START.items():
        if a in ix.size:
            out[a] = (name, 'library', 'start-code')
    bind = bind_section(ix)
    if bind:
        for a in addrs:
            if bind[0] <= int(a, 16) < bind[1]:
                label = 'steamstub_entry' if ix.name[a] == 'entry' else f'steamstub_{a}'
                out[a] = (label, 'library', 'steam-stub')

    rejected = sorted(cands - keep - set(START), key=lambda a: int(a, 16))
    false_fid = sorted((a for a in addrs if k[a] == 'fid' and a not in lib),
                       key=lambda a: int(a, 16))
    return ix, out, rejected, false_fid


def render(rows):
    lines = [HEADER]
    for a in sorted(rows, key=lambda a: int(a, 16)):
        label, kind, evidence = rows[a]
        lines.append(f'{a}\t{label}\t{kind}\t{evidence}\n')
    return ''.join(lines)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument('--out', default=OUT)
    ap.add_argument('--check', help='exit 1 if this file differs from what would be written')
    ap.add_argument('--report', action='store_true', help='print counts and the doubtful cases')
    args = ap.parse_args()
    rows = {}
    for binary in BINARIES:
        if not os.path.exists(os.path.join(ROOT, 'analysis', 'export', binary, 'functions.tsv')):
            print(f'{binary}: no Ghidra export, skipped', file=sys.stderr)
            continue
        ix, out, rejected, false_fid = classify(binary)
        rows.update(out)
        by = {}
        for label, kind, evidence in out.values():
            n, b = by.get(evidence, (0, 0))
            by[evidence] = (n + 1, b)
        for a, (label, kind, evidence) in out.items():
            n, b = by[evidence]
            by[evidence] = (n, b + ix.size.get(a, 0))
        summary = ', '.join(f'{e} {n} ({b:,} bytes)' for e, (n, b) in sorted(by.items()))
        print(f'{binary}: {summary}', file=sys.stderr)
        if args.report:
            for a in rejected:
                print(f'  not library (calls the game): {a} {ix.size[a]} bytes', file=sys.stderr)
            for a in false_fid:
                print(f'  Function ID match outside the library: {a} {ix.name[a]}', file=sys.stderr)
    text = render(rows)
    if args.check:
        old = open(args.check, encoding='utf-8').read() if os.path.exists(args.check) else ''
        if old != text:
            print(f'{args.check} is out of date: run library.py again', file=sys.stderr)
            return 1
        return 0
    with open(args.out, 'w', encoding='utf-8') as f:
        f.write(text)
    print(f'wrote {len(rows)} rows to {os.path.relpath(args.out, ROOT)}', file=sys.stderr)
    return 0


if __name__ == '__main__':
    sys.exit(main())
