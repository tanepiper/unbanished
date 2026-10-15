"""Compare two random-draw traces: the original (tools/emu/rng_trace.py) against the port
(banished_game::random::trace). Each line is `kind<TAB>caller`; only the kinds and arguments
are compared, the callers give context.

  python3 tools/triage/rngdiff.py original.rng port.rng [--context 6]

Exit 0 when the sequences match. Otherwise prints the first divergence with the draws around
it from both sides, which usually names the branch where the port draws in another order,
draws one too many, or misses one.
"""
import argparse
import sys


def load(path):
    rows = []
    for line in open(path):
        line = line.rstrip('\n')
        if line:
            kind, _, caller = line.partition('\t')
            rows.append((kind, caller))
    return rows


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    p.add_argument('original')
    p.add_argument('port')
    p.add_argument('--context', type=int, default=6)
    args = p.parse_args()
    a, b = load(args.original), load(args.port)
    n = min(len(a), len(b))
    first = next((i for i in range(n) if a[i][0] != b[i][0]), None)
    if first is None and len(a) == len(b):
        print(f'identical: {len(a)} draws')
        return 0
    if first is None:
        first = n
        print(f'same first {n} draws; original has {len(a)}, port has {len(b)}')
    else:
        print(f'diverge at draw {first}: original {a[first][0]} vs port {b[first][0]}')
    lo, hi = max(0, first - args.context), first + args.context
    width = max([len(k) for k, _ in a[lo:hi] + b[lo:hi]] + [4])
    print(f'{"#":>5}  {"original":<{width}}  {"port":<{width}}  callers')
    for i in range(lo, hi):
        ka, ca = a[i] if i < len(a) else ('-', '')
        kb, cb = b[i] if i < len(b) else ('-', '')
        mark = '>' if i == first else ' '
        print(f'{mark}{i:4d}  {ka:<{width}}  {kb:<{width}}  {ca}  |  {cb}')
    return 1


if __name__ == '__main__':
    sys.exit(main())
