"""Ground truth for the gameplay map and town start point.

Usage: .venv/bin/python tools/emu/start_point.py DIR [RADIUS]
Reads DIR/grid_5.f32 and writes DIR/map_types.u8 (FUN_1400a3790 over the n x n gameplay map,
n = terrain size / 2) and DIR/start.txt (FUN_1400a7bf0 with the profile's start radius, 13).
"""
import math
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(__file__))
from pe_emu import Emu  # noqa: E402


def main(out, radius=13):
    radius = int(radius)
    grid = open(os.path.join(out, 'grid_5.f32'), 'rb').read()
    floats = struct.unpack(f'<{len(grid) // 4}f', grid)
    width = math.isqrt(len(floats))
    size = width - 1
    n = size // 2
    heights = struct.pack(f'<{len(floats)}h',
                          *[(int(v * 256.0) + 0x8000) % 0x10000 - 0x8000 for v in floats])
    emu = Emu('game-data/original/Application-steam-x64.exe', 'game-data/original/Runtime-steam-x64.dll')
    comp = emu.malloc(0x200)
    hp = emu.malloc(len(heights))
    emu.write(hp, heights)
    emu.write(comp + 0xc0, struct.pack('<6i', size, size // 2, 32, 33, width, 33 * 33))
    emu.write(comp + 0xd8, struct.pack('<Q', hp))
    game = emu.malloc(0x400)
    world = emu.malloc(0x200)
    emu.write(game + 0x110, struct.pack('<Q', comp))
    emu.write(game + 0xf8, struct.pack('<Q', world))
    emu.write(0x1401cbdd8, struct.pack('<Q', game))
    cells = emu.malloc(0x30 * n * n)
    emu.write(world, struct.pack('<i', n))
    emu.write(world + 0x80, struct.pack('<Q', cells))
    types = bytearray()
    for cell in range(n * n):
        t = emu.call(0x1400a3790, world, cell) & 0xff
        types.append(t)
        emu.write(cells + 0x30 * cell + 0x29, bytes([t]))
    with open(os.path.join(out, 'map_types.u8'), 'wb') as fh:
        fh.write(types)
    xy = emu.malloc(8)
    emu.call(0x1400a7bf0, 0, xy, xy + 4, radius, radius * radius)
    x, y = struct.unpack('<2i', emu.read(xy, 8))
    with open(os.path.join(out, 'start.txt'), 'w') as fh:
        fh.write(f'{radius} {x} {y}\n')
    print('map', n, 'start', x, y)


if __name__ == '__main__':
    main(*sys.argv[1:])
