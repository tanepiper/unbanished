"""Ground truth for the coarse path graph and path-rank distance (FUN_1400adb40/FUN_1400adf60,
FUN_1400ae160, FUN_1400a4ce0).

Usage: .venv/bin/python tools/emu/sectors.py OUT_DIR

For 12 random 64x64 maps: build the graph, dump every cell's node id (+0x18), run 150 distance
queries; then turn 40 random cells to water (type 8), mark them (FUN_1400ae160), re-label regions
and run 150 more queries (rebuilding lazily inside the queries). Writes `OUT_DIR/sectors.tsv`:
`map types(hex) nodes(comma)`, `q a b distance`, `change cell`, `nodes2 ...`.
"""
import os
import random
import struct
import sys

sys.path.insert(0, 'tools/emu')
from pe_emu import Emu  # noqa: E402

BUILD, REBUILD, MARK, DIST, LABEL = 0x1400adb40, 0x1400adf60, 0x1400ae160, 0x1400a4ce0, 0x1400a1950
N = 64


def main(out_dir):
    os.makedirs(out_dir, exist_ok=True)
    emu = Emu('game-data/original/Application-steam-x64.exe',
              'game-data/original/Runtime-steam-x64.dll')
    rng = random.Random(5)
    lines = []
    for _ in range(12):
        p = rng.uniform(0.1, 0.45)
        types = [rng.choice([5, 6, 7, 8]) if rng.random() < p else rng.choice([0, 1, 2, 2, 10])
                 for _ in range(N * N)]
        cells = emu.malloc(0x30 * N * N)
        emu.write(cells, b''.join(bytes(0x29) + bytes([t]) + bytes(6) for t in types))
        m = emu.malloc(0x200)
        emu.write(m, bytes(0x200))
        emu.write(m, struct.pack('<i', N))
        emu.write(m + 0x80, struct.pack('<Qi', cells, N * N))
        emu.write(m + 0xa2, bytes([1]))
        emu.write(m + 0xa8 + 0x28, struct.pack('<ii', 8, 0x18))
        g = emu.malloc(0x108)
        emu.write(g, bytes(0x108))
        emu.call(BUILD, g, m, 0, 0, N // 8, N // 8, timeout_insns=500_000_000)
        emu.call(REBUILD, g, timeout_insns=500_000_000)
        cache = emu.malloc(0x38)
        emu.write(cache, bytes(0x38))
        emu.write(m + 0x70, struct.pack('<QQ', g, cache))
        node = lambda i: struct.unpack_from('<i', emu.read(cells + 0x30 * i + 0x18, 4))[0]
        lines.append(f'map\t{bytes(types).hex()}\t{",".join(str(node(i)) for i in range(N * N))}')

        def queries():
            for _ in range(150):
                a, b = rng.randrange(N * N), rng.randrange(N * N)
                d = emu.call(DIST, m, a, b, 0, timeout_insns=500_000_000) & 0xffffffff
                d = d - (1 << 32) if d >= 1 << 31 else d
                lines.append(f'q\t{a}\t{b}\t{d}')
        queries()
        for _ in range(40):
            c = rng.randrange(N * N)
            emu.write(cells + 0x30 * c + 0x29, bytes([8]))
            emu.call(MARK, g, c % N, c // N)
            lines.append(f'change\t{c}')
        emu.write(m + 0xa2, bytes([1]))
        queries()
        lines.append(f'nodes2\t{",".join(str(node(i)) for i in range(N * N))}')
    with open(os.path.join(out_dir, 'sectors.tsv'), 'w') as f:
        f.write('\n'.join(lines) + '\n')
    print(f'wrote {len(lines)} lines')


if __name__ == '__main__':
    main(sys.argv[1])
