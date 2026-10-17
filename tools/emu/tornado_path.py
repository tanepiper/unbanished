"""Ground truth for the tornado's line (`FUN_1400592e0`): the midpoint of each span moved off
it by `RandFloat` (the game Random, `Game +0xd8`), then each half, depth first.

Usage: .venv/bin/python tools/emu/tornado_path.py OUT_DIR [COUNT]

Writes `OUT_DIR/tornado_path.tsv`: `seed x0 y0 x1 y1` (the ends, z 0) and then the 129 points as
hex f32 bits, `x,y,z` each.
"""
import os
import random
import struct
import sys

sys.path.insert(0, 'tools/emu')
from pe_emu import Emu  # noqa: E402

DISPLACE = 0x1400592e0
POINTS = 129


def h(x):
    return '%08x' % struct.unpack('<I', struct.pack('<f', x))[0]


def main(out_dir, count=300):
    os.makedirs(out_dir, exist_ok=True)
    emu = Emu('game-data/original/Application-steam-x64.exe',
              'game-data/original/Runtime-steam-x64.dll')
    game = emu.malloc(0x160)
    rnd = emu.malloc(2504)
    emu.write(0x1401cbdd8, struct.pack('<Q', game))
    emu.write(game + 0xd8, struct.pack('<Q', rnd))
    path = emu.malloc(POINTS * 12)
    rng = random.Random(9)
    rows = []
    for _ in range(int(count)):
        seed = rng.randrange(1, 1 << 31)
        ends = [float(rng.randrange(-256, 256)) + rng.choice([0.0, 0.5]) for _ in range(4)]
        emu.call(emu.exports['??0Random@System@@QEAA@I@Z'], rnd, seed)
        emu.write(path, b'\0' * POINTS * 12)
        emu.write(path, struct.pack('<3f', ends[0], ends[1], 0.0))
        emu.write(path + 12 * (POINTS - 1), struct.pack('<3f', ends[2], ends[3], 0.0))
        emu.call(DISPLACE, 0, path, 0, POINTS - 1, timeout_insns=50_000_000)
        pts = struct.unpack('<%df' % (POINTS * 3), emu.read(path, POINTS * 12))
        rows.append(' '.join([str(seed)] + [repr(e) for e in ends]) + '\t'
                    + ' '.join(','.join(h(pts[3 * i + k]) for k in range(3))
                               for i in range(POINTS)))
    with open(os.path.join(out_dir, 'tornado_path.tsv'), 'w') as f:
        f.write('\n'.join(rows) + '\n')
    print(f'wrote {len(rows)} lines')


if __name__ == '__main__':
    main(sys.argv[1], *sys.argv[2:])
