"""Triage the original executable's functions: what calls what, what each touches, and which
ones the project has already traced (mentioned in docs/ or the Rust code).

Reads the local Ghidra export (analysis/export/<binary>/decomp.c and functions.tsv; never
distributed). Prints derived facts only (addresses, sizes, names, string literals).

Usage:
  python3 tools/triage/fnindex.py info 0x1401148b0        # size, callers, callees, strings, types
  python3 tools/triage/fnindex.py frontier 0x1401146c0 --depth 2
                                                        # untraced functions reachable from here
  python3 tools/triage/fnindex.py find CitizenAIComponent  # functions using a string/type name
  python3 tools/triage/fnindex.py coverage                 # traced share overall and by type
  python3 tools/triage/fnindex.py hot --limit 30           # untraced functions most called by
                                                        # traced ones (good next targets)
Options: --binary Application-steam-x64.exe (default) or Runtime-steam-x64.dll.
"""
import argparse
import collections
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
HEADER = re.compile(r'^// ==== ([0-9a-f]+) (\S+)')
CALL = re.compile(r'\bFUN_(14[0-9a-f]{7}|18[0-9a-f]{7})\b')
DAT = re.compile(r'\bDAT_(14[0-9a-f]{7}|18[0-9a-f]{7})\b')
STRING = re.compile(r'"((?:[^"\\]|\\.){3,80})"')
TYPEINFO = re.compile(r'&DAT_([0-9a-f]+),"([A-Za-z_][A-Za-z0-9_]*)","([A-Za-z_][A-Za-z0-9_:]*)"')
MENTION = re.compile(r'(?<![0-9a-fA-F])(?:0x|FUN_|DAT_)?(1[48][0-9a-f]{7})(?![0-9a-fA-F])')


class Index:
    def __init__(self, binary):
        base = os.path.join(ROOT, 'analysis', 'export', binary)
        self.size, self.name = {}, {}
        with open(os.path.join(base, 'functions.tsv')) as f:
            for line in f:
                cols = line.rstrip('\n').split('\t')
                if len(cols) >= 3:
                    self.size[cols[0]] = int(cols[1] or 0)
                    self.name[cols[0]] = cols[2]
        self.callees = collections.defaultdict(set)
        self.callers = collections.defaultdict(set)
        self.strings = collections.defaultdict(set)
        self.dats = collections.defaultdict(set)
        self.types = {}
        self.derived = set()
        current = None
        with open(os.path.join(base, 'decomp.c'), errors='replace') as f:
            for line in f:
                m = HEADER.match(line)
                if m:
                    current = m.group(1)
                    if m.group(2) and not m.group(2).startswith('FUN_'):
                        self.name.setdefault(current, m.group(2))
                    continue
                if current is None:
                    continue
                for t in TYPEINFO.finditer(line):
                    self.types[t.group(1)] = t.group(2)
                for c in CALL.findall(line):
                    if c != current:
                        self.callees[current].add(c)
                        self.callers[c].add(current)
                for d in DAT.findall(line):
                    self.dats[current].add(d)
                for s in STRING.findall(line):
                    self.strings[current].add(s)
                if 'IsDerivedFrom' in line:
                    self.derived.add(current)
        self.traced = self._mentions()

    def _mentions(self):
        """Addresses named anywhere in docs/, crates/, apps/ or tools/ (the project's own
        record of what has been traced)."""
        found = collections.defaultdict(set)
        for top in ('docs', 'crates', 'apps', 'tools'):
            for dirpath, _, files in os.walk(os.path.join(ROOT, top)):
                if 'target' in dirpath or '__pycache__' in dirpath:
                    continue
                for fn in files:
                    if not fn.endswith(('.md', '.rs', '.py', '.wgsl', '.tsv')):
                        continue
                    path = os.path.join(dirpath, fn)
                    try:
                        text = open(path, errors='replace').read()
                    except OSError:
                        continue
                    rel = os.path.relpath(path, ROOT)
                    for a in MENTION.findall(text):
                        found[a].add(rel)
        return found

    def type_names(self, fn):
        return sorted({self.types[d] for d in self.dats.get(fn, ()) if d in self.types})

    def accessor(self, fn):
        """Small functions that scan the owner's components for one type (`GetComponent<T>`)
        or cast a description: name them after the type."""
        types = self.type_names(fn)
        if fn in self.derived and len(types) == 1 and self.size.get(fn, 0) <= 200:
            return f'get<{types[0]}>'
        return None

    def label(self, fn):
        n = self.name.get(fn, f'FUN_{fn}')
        if self.accessor(fn):
            n = f'{n} = {self.accessor(fn)}'

        mark = 'traced' if fn in self.traced else 'untraced'
        return f'{fn} {n} ({self.size.get(fn, 0)} bytes, {mark})'


def norm(a):
    a = a.lower().removeprefix('0x').removeprefix('fun_')
    return a


def cmd_info(ix, args):
    fn = norm(args.address)
    print(ix.label(fn))
    if fn in ix.traced:
        print('  mentioned in:', ', '.join(sorted(ix.traced[fn])[:8]))
    types = ix.type_names(fn)
    if types:
        print('  types:', ', '.join(types))
    if ix.strings.get(fn):
        print('  strings:', ', '.join(repr(s) for s in sorted(ix.strings[fn])[:12]))
    for title, group in (('callers', ix.callers), ('callees', ix.callees)):
        items = sorted(group.get(fn, ()))
        print(f'  {title} ({len(items)}):')
        for c in items[:40]:
            print('    ', ix.label(c))


def cmd_frontier(ix, args):
    """Breadth-first over callees; report untraced functions with their depth, size and hints."""
    start = norm(args.address)
    seen = {start: 0}
    queue = collections.deque([start])
    rows = []
    while queue:
        fn = queue.popleft()
        d = seen[fn]
        if d >= args.depth:
            continue
        for c in sorted(ix.callees.get(fn, ())):
            if c in seen:
                continue
            seen[c] = d + 1
            queue.append(c)
            if (c not in ix.traced and not ix.name.get(c, '').startswith('RUNTIME')
                    and not ix.accessor(c)):
                rows.append((d + 1, -ix.size.get(c, 0), c))
    rows.sort()
    print(f'{len(rows)} untraced functions within depth {args.depth} of {start}:')
    for d, neg, c in rows[:args.limit]:
        hint = ', '.join(ix.type_names(c)[:3] + sorted(ix.strings.get(c, ()))[:2])
        print(f'  d{d} {c} {-neg:6d} bytes  {len(ix.callers.get(c, ()))} callers  {hint}')


def cmd_find(ix, args):
    term = args.term
    dats = {d for d, n in ix.types.items() if n == term}
    hits = [fn for fn in ix.size
            if any(term in s for s in ix.strings.get(fn, ())) or ix.dats.get(fn, set()) & dats]
    hits.sort(key=lambda f: (f in ix.traced, -ix.size.get(f, 0)))
    print(f'{len(hits)} functions use {term!r} (untraced first):')
    for fn in hits[:args.limit]:
        print('  ', ix.label(fn))


def cmd_coverage(ix, args):
    fns = [f for f in ix.size if not ix.name.get(f, '').startswith('RUNTIME')]
    traced = [f for f in fns if f in ix.traced]
    total_b = sum(ix.size[f] for f in fns)
    traced_b = sum(ix.size[f] for f in traced)
    print(f'{len(traced)}/{len(fns)} functions traced '
          f'({traced_b * 100 / max(total_b, 1):.1f}% of code bytes)')
    by_type = collections.Counter()
    by_type_traced = collections.Counter()
    for f in fns:
        for t in ix.type_names(f):
            by_type[t] += ix.size[f]
            if f in ix.traced:
                by_type_traced[t] += ix.size[f]
    print('by class type touched (bytes traced / total), least covered first:')
    rows = sorted(by_type, key=lambda t: (by_type_traced[t] / by_type[t], -by_type[t]))
    for t in rows[:args.limit]:
        print(f'  {t:32s} {by_type_traced[t]:7d} / {by_type[t]:7d}')


def cmd_hot(ix, args):
    """Untraced functions ranked by how many traced functions call them."""
    score = collections.Counter()
    for f in ix.traced:
        for c in ix.callees.get(f, ()):
            if (c not in ix.traced and not ix.name.get(c, '').startswith('RUNTIME')
                    and not ix.accessor(c)):
                score[c] += 1
    print('untraced functions called most by traced code:')
    for c, n in score.most_common(args.limit):
        hint = ', '.join(ix.type_names(c)[:3] + sorted(ix.strings.get(c, ()))[:2])
        print(f'  {c} {ix.size.get(c, 0):6d} bytes  from {n} traced  {hint}')


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    p.add_argument('--binary', default='Application-steam-x64.exe')
    sub = p.add_subparsers(dest='cmd', required=True)
    s = sub.add_parser('info')
    s.add_argument('address')
    s = sub.add_parser('frontier')
    s.add_argument('address')
    s.add_argument('--depth', type=int, default=2)
    s.add_argument('--limit', type=int, default=40)
    s = sub.add_parser('find')
    s.add_argument('term')
    s.add_argument('--limit', type=int, default=40)
    s = sub.add_parser('coverage')
    s.add_argument('--limit', type=int, default=30)
    s = sub.add_parser('hot')
    s.add_argument('--limit', type=int, default=30)
    args = p.parse_args()
    ix = Index(args.binary)
    {'info': cmd_info, 'frontier': cmd_frontier, 'find': cmd_find,
     'coverage': cmd_coverage, 'hot': cmd_hot}[args.cmd](ix, args)


if __name__ == '__main__':
    sys.exit(main())
