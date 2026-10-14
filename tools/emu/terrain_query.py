"""Ground truth for the gameplay terrain queries.

Usage: .venv/bin/python tools/emu/terrain_query.py DIR
Reads DIR/grid_5.f32 (from terrain_stages.py) and writes:
- DIR/cell_types.u8: FUN_1400a3790 (natural cell type) for every map cell, row-major;
- DIR/height_samples.bin: 4096 records of (x, y, height, normal xyz) as f32 from FUN_1400b6460 at
  pseudo-random fractional map positions (the positions are part of the file).
"""
import math
import os
import random
import struct
import sys

sys.path.insert(0, os.path.dirname(__file__))
from pe_emu import Emu, F32  # noqa: E402,F401


def main(out):
    grid = open(os.path.join(out, 'grid_5.f32'), 'rb').read()
    floats = struct.unpack(f'<{len(grid) // 4}f', grid)
    width = math.isqrt(len(floats))
    size = width - 1
    heights = struct.pack(f'<{len(floats)}h',
                          *[(int(v * 256.0) + 0x8000) % 0x10000 - 0x8000 for v in floats])
    emu = Emu('game-data/original/Application-steam-x64.exe', 'game-data/original/Runtime-steam-x64.dll')
    comp = emu.malloc(0x200)
    hp = emu.malloc(len(heights))
    emu.write(hp, heights)
    emu.write(comp + 0xc0, struct.pack('<6i', size, size // 2, 32, 33, width, 33 * 33))
    emu.write(comp + 0xd8, struct.pack('<Q', hp))
    game = emu.malloc(0x400)
    emu.write(game + 0x110, struct.pack('<Q', comp))
    emu.write(0x1401cbdd8, struct.pack('<Q', game))
    world = emu.malloc(0x200)  # map manager: *map = cells per side
    emu.write(world, struct.pack('<i', size))

    types = bytearray()
    for cell in range(size * size):
        types.append(emu.call(0x1400a3790, world, cell) & 0xff)
    with open(os.path.join(out, 'cell_types.u8'), 'wb') as fh:
        fh.write(types)
    print('cell types', {t: types.count(t) for t in sorted(set(types))})

    rng = random.Random(1)
    pos = emu.malloc(16)
    normal = emu.malloc(16)
    with open(os.path.join(out, 'height_samples.bin'), 'wb') as fh:
        for _ in range(4096):
            x = struct.unpack('<f', struct.pack('<f', rng.uniform(-size / 2, size / 2)))[0]
            y = struct.unpack('<f', struct.pack('<f', rng.uniform(-size / 2, size / 2)))[0]
            emu.write(pos, struct.pack('<3f', x, y, 0.0))
            emu.write(normal, bytes(12))
            h = emu.call_f32(0x1400b6460, comp, pos, normal)
            fh.write(struct.pack('<3f', x, y, h) + emu.read(normal, 12))
    print('height samples written')


if __name__ == '__main__':
    main(sys.argv[1])
