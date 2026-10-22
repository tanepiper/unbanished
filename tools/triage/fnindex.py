"""Triage the original executable's functions: what calls what, what each touches, and which
ones the project has already traced (mentioned in docs/ or the Rust code).

Reads the local Ghidra export (analysis/export/<binary>/decomp.c and functions.tsv; never
distributed). Prints derived facts only (addresses, sizes, names, string literals).

Usage:
  python3 tools/triage/fnindex.py info 0x1401148b0        # size, callers, callees, strings, types
  python3 tools/triage/fnindex.py frontier 0x1401146c0 --depth 2
                                                        # untraced functions reachable from here
  python3 tools/triage/fnindex.py find CitizenAIComponent  # functions using a string/type name
  python3 tools/triage/fnindex.py coverage                 # the coverage figure (traced = labelled),
                                                        # library code apart, and by type
  python3 tools/triage/fnindex.py hot --limit 30           # untraced functions most called by
                                                        # traced ones (good next targets)
Options: --binary Application-steam-x64.exe (default) or Runtime-steam-x64.dll.
"""
import argparse
import collections
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from ghidra_index import Index, norm  # noqa: E402


def cmd_info(ix, args):
    fn = norm(args.address)
    print(ix.describe(fn))
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
            print('    ', ix.describe(c))


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
            if (c not in ix.traced and ix.category(c) == 'game'
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
        print('  ', ix.describe(fn))


def cmd_coverage(ix, args):
    """The project's one coverage figure. A function is traced when it has a label: a hand label
    in labels/names.tsv (with its evidence doc) or a label from a ported item's `Original:`
    citation in labels/names-from-port.tsv. The share is of the binary's own code by size, with
    library code (labels/library.tsv), exception funclets and thunks counted apart."""
    groups = collections.defaultdict(list)
    for f in ix.size:
        groups[ix.category(f)].append(f)
    own = groups['game']
    traced = [f for f in own if f in ix.labelled]
    cited = [f for f in own if f not in ix.labelled and f in ix.traced]
    total_b = sum(ix.size[f] for f in own)

    def share(fns):
        b = sum(ix.size[f] for f in fns)
        return f'{len(fns):,} functions, {b:,} bytes ({b * 100 / max(total_b, 1):.1f} %)'

    print(f'{ix.binary}: {len(ix.size):,} functions')
    print(f'  own code: {len(own):,} functions, {total_b:,} bytes')
    print(f'    traced (labelled): {share(traced)}')
    print(f'    cited in code or docs, no label yet: {share(cited)}')
    print(f'    not traced: {share([f for f in own if f not in ix.labelled and f not in ix.traced])}')
    evidence = collections.Counter(ix.library[f][2] for f in groups['library'])
    lib_b = sum(ix.size[f] for f in groups['library'])
    print(f'  library code: {len(groups["library"]):,} functions, {lib_b:,} bytes ('
          + ', '.join(f'{e} {n}' for e, n in sorted(evidence.items())) + ')')
    for g, title in (('handler', 'exception funclets'), ('thunk', 'thunks')):
        print(f'  {title}: {len(groups[g]):,} functions, {sum(ix.size[f] for f in groups[g]):,} bytes')
    if args.limit <= 0:
        return
    by_type = collections.Counter()
    by_type_traced = collections.Counter()
    for f in own:
        for t in ix.type_names(f):
            by_type[t] += ix.size[f]
            if f in ix.labelled:
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
            if (c not in ix.traced and ix.category(c) == 'game'
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
