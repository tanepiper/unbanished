"""Ground truth for the CitizenAI candidate sort (FUN_140030770, the inlined CRT qsort).

Usage: .venv/bin/python tools/emu/candidate_sort.py OUT_DIR

Sorts 400 pseudo-random candidate arrays (lengths 2..24, priorities 0..7, entries
`(activity, priority, 0)`) with the original function and writes `OUT_DIR/candidate_sort.tsv`
(`input` and `output` as `activity:priority` lists).
"""
import os
import random
import struct
import sys

sys.path.insert(0, 'tools/emu')
from pe_emu import Emu  # noqa: E402

SORT = 0x140030770


def main(out_dir):
    os.makedirs(out_dir, exist_ok=True)
    emu = Emu('game-data/original/Application-steam-x64.exe',
              'game-data/original/Runtime-steam-x64.dll')
    rng = random.Random(1234)
    rows = []
    for _ in range(400):
        n = rng.randint(2, 24)
        items = [(rng.randint(0, 15), rng.randint(0, 7)) for _ in range(n)]
        buf = emu.malloc(12 * n)
        emu.write(buf, b''.join(struct.pack('<iii', a, p, 0) for a, p in items))
        scratch = emu.malloc(64)
        emu.call(SORT, buf, n, scratch, timeout_insns=5_000_000)
        out = [struct.unpack_from('<ii', emu.read(buf + 12 * i, 8)) for i in range(n)]
        fmt = lambda xs: ','.join(f'{a}:{p}' for a, p in xs)
        rows.append(f'{fmt(items)}\t{fmt(out)}')
    with open(os.path.join(out_dir, 'candidate_sort.tsv'), 'w') as f:
        f.write('\n'.join(rows) + '\n')
    print(f'wrote {len(rows)} sorts')


if __name__ == '__main__':
    main(sys.argv[1])
