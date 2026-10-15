"""Ground truth for the map A* (grid init FUN_1400ab5e0, search FUN_14001d760).

Usage: .venv/bin/python tools/emu/astar.py OUT_DIR

Builds 30 random 32x32 grids (cell types 0,1,2,3,8,10 and some blocked water), sets the citizen
cost table (FUN_1400a3a20 mode 0), mask 0xe1f and diagonal steps, then runs 20 searches per
grid on the same solver (so search ids advance). Writes `OUT_DIR/astar.tsv`:
`grid types(hex)`, then `from to path` rows (`-` for no path).
"""
import os
import random
import struct
import sys

sys.path.insert(0, 'tools/emu')
from pe_emu import Emu  # noqa: E402

INIT, FIND = 0x1400ab5e0, 0x14001d760
COSTS = {0: 3, 1: 3, 2: 4, 3: 8, 4: 16, 5: 16, 6: 16, 7: 16, 8: 16, 9: 3, 10: 4}
N = 32


def main(out_dir):
    os.makedirs(out_dir, exist_ok=True)
    emu = Emu('game-data/original/Application-steam-x64.exe',
              'game-data/original/Runtime-steam-x64.dll')
    rng = random.Random(99)
    lines = []
    out = emu.malloc(0x4000 * 4)
    for g in range(30):
        types = [rng.choice([2, 2, 2, 2, 10, 10, 1, 0, 3, 8]) for _ in range(N * N)]
        grid = emu.malloc(0x100)
        emu.write(grid, bytes(0x100))
        emu.call(INIT, grid, N, timeout_insns=50_000_000)
        cells = emu.u64(grid + 0xe0)
        emu.write(cells, bytes(types))
        for t, c in COSTS.items():
            emu.write(grid + 0x50 + 4 * t, struct.pack('<i', c))
        emu.write(grid + 0xd0, struct.pack('<I', 0xe1f))
        emu.write(grid + 0xd4, bytes([1, 0, 0]))
        lines.append(f'grid\t{bytes(types).hex()}')
        for _ in range(20):
            a, b = rng.randrange(N * N), rng.randrange(N * N)
            n = emu.call(FIND, grid, a, b, 0, out, timeout_insns=200_000_000) & 0xffffffff
            path = [emu.i32(out + 4 * i) for i in range(n)] if n else []
            lines.append(f'{a}\t{b}\t{",".join(map(str, path)) if path else "-"}')
    with open(os.path.join(out_dir, 'astar.tsv'), 'w') as f:
        f.write('\n'.join(lines) + '\n')
    print(f'wrote {len(lines)} lines')


if __name__ == '__main__':
    main(sys.argv[1])
