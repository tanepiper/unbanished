"""Ground truth for map connectivity regions (FUN_1400a1950 → FUN_1400a1640).

Usage: .venv/bin/python tools/emu/regions.py OUT_DIR

Builds 20 random 40x40 maps (48-byte cells, type at +0x29) with water/blocking types mixed in,
marks regions dirty (+0xa2) and runs the labelling. Writes `OUT_DIR/regions.tsv`:
`types(hex) regions(comma list)`.
"""
import os
import random
import struct
import sys

sys.path.insert(0, 'tools/emu')
from pe_emu import Emu  # noqa: E402

LABEL = 0x1400a1950
N = 40


def main(out_dir):
    os.makedirs(out_dir, exist_ok=True)
    emu = Emu('game-data/original/Application-steam-x64.exe',
              'game-data/original/Runtime-steam-x64.dll')
    rng = random.Random(7)
    lines = []
    for _ in range(20):
        p = rng.uniform(0.2, 0.55)
        types = [rng.choice([5, 6, 7, 8]) if rng.random() < p else rng.choice([0, 1, 2, 10, 11])
                 for _ in range(N * N)]
        cells = emu.malloc(0x30 * N * N)
        emu.write(cells, b''.join(bytes(0x29) + bytes([t]) + bytes(6) for t in types))
        m = emu.malloc(0x200)
        emu.write(m, bytes(0x200))
        emu.write(m, struct.pack('<i', N))
        emu.write(m + 0x80, struct.pack('<Qi', cells, N * N))
        emu.write(m + 0xa2, bytes([1]))
        # Fill-queue pool (list at +0xa8): initial block count 8, 24-byte nodes.
        emu.write(m + 0xa8 + 0x28, struct.pack('<ii', 8, 0x18))
        emu.call(LABEL, m, timeout_insns=200_000_000)
        regions = [struct.unpack_from('<H', emu.read(cells + 0x30 * i + 0x26, 2))[0]
                   for i in range(N * N)]
        lines.append(f'{bytes(types).hex()}\t{",".join(map(str, regions))}')
    with open(os.path.join(out_dir, 'regions.tsv'), 'w') as f:
        f.write('\n'.join(lines) + '\n')
    print(f'wrote {len(lines)} maps')


if __name__ == '__main__':
    main(sys.argv[1])
