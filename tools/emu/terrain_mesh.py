"""Ground truth for the terrain tile vertex builder (FUN_1400b1d50).

Usage: .venv/bin/python tools/emu/terrain_mesh.py DIR
Reads DIR/grid_5.f32 (from terrain_stages.py), converts it to the saved i16 heights, and writes
DIR/tiles.bin: the 33x33 vertices (16 bytes each) of every 32x32-cell tile, row-major by tile.
"""
import math
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(__file__))
from pe_emu import Emu  # noqa: E402


def main(out):
    grid = open(os.path.join(out, 'grid_5.f32'), 'rb').read()
    floats = struct.unpack(f'<{len(grid) // 4}f', grid)
    width = math.isqrt(len(floats))
    size = width - 1
    heights = struct.pack(f'<{len(floats)}h', *[((int(v * 256.0)) + 0x8000) % 0x10000 - 0x8000 for v in
                                                 (struct.unpack('<f', struct.pack('<f', f))[0] for f in floats)])
    emu = Emu('game-data/original/Application-steam-x64.exe', 'game-data/original/Runtime-steam-x64.dll')
    comp = emu.malloc(0x200)
    hp = emu.malloc(len(heights))
    emu.write(hp, heights)
    emu.write(comp + 0xc0, struct.pack('<6i', size, size // 2, 32, 33, width, 33 * 33))
    emu.write(comp + 0xd8, struct.pack('<Q', hp))
    buf = emu.malloc(33 * 33 * 16)
    tiles = size // 32
    with open(os.path.join(out, 'tiles.bin'), 'wb') as fh:
        for ty in range(tiles):
            for tx in range(tiles):
                emu.call(0x1400b1d50, comp, 32 * tx, 32 * ty, 33, 33, buf)
                fh.write(emu.read(buf, 33 * 33 * 16))
    print('tiles', tiles * tiles)


if __name__ == '__main__':
    main(sys.argv[1])
