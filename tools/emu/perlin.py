"""Perlin noise from the original Runtime.

Usage: .venv/bin/python tools/emu/perlin.py [CASES]

Builds `Utility::PerlinNoise(persistence, octaves, seed x, seed y)` and calls
`PerlinNoise::Generate(x, y)` for CASES points (default 4000): the temperature's settings (0.25,
4) and others (persistences 0 to 1, 1 to 8 octaves, a fractional count among them), seeds over the
whole int range, and coordinates that are negative, fractional, whole and large. Writes
game-data/derived/emu/perlin/cases.tsv with every float as its bits in hex: persistence,
octaves, the two seeds, x, y, then the value.
"""
import os
import random
import struct
import sys

sys.path.insert(0, os.path.dirname(__file__))
from pe_emu import Emu, F32  # noqa: E402

OUT = 'game-data/derived/emu/perlin'


def f32(v):
    return struct.unpack('<f', struct.pack('<f', v))[0]


def bits(v):
    return struct.unpack('<I', struct.pack('<f', v))[0]


def main():
    cases = int(sys.argv[1]) if len(sys.argv) > 1 else 4000
    e = Emu('game-data/original/Application-steam-x64.exe', 'game-data/original/Runtime-steam-x64.dll')
    e.hook_function(0x1800cd0d0, 'platform counter setup', lambda a: 1)
    e.call(0x1800cd030)
    X = e.exports
    pn = e.malloc(16)
    r = random.Random(0x18010b500)
    os.makedirs(OUT, exist_ok=True)
    with open(f'{OUT}/cases.tsv', 'w') as out:
        out.write('persistence\toctaves\tseed_x\tseed_y\tx\ty\tvalue\n')
        for k in range(cases):
            if k % 2 == 0:
                p, o = 0.25, 4.0
            else:
                p, o = f32(r.uniform(0.0, 1.0)), f32(r.choice([1.0, 2.0, 3.0, 5.0, 8.0, r.uniform(1.0, 6.0)]))
            sx, sy = r.randrange(-(1 << 31), 1 << 31), r.randrange(-(1 << 31), 1 << 31)
            x = f32(r.choice([r.uniform(-100, 100), r.uniform(0, 3600 / 75.0), float(r.randrange(-50, 50)),
                              r.uniform(-1e4, 1e4)]))
            y = f32(r.choice([r.uniform(-100, 100), 0.0, float(r.randrange(-50, 50)), r.uniform(-1e4, 1e4)]))
            e.call(X['??0PerlinNoise@Utility@@QEAA@MMHH@Z'], pn, F32(p), F32(o), sx & 0xffffffff, sy & 0xffffffff)
            v = e.call_f32(X['?Generate@PerlinNoise@Utility@@QEBAMMM@Z'], pn, F32(x), F32(y))
            row = [bits(p), bits(o), sx & 0xffffffff, sy & 0xffffffff, bits(x), bits(y), bits(v)]
            out.write('\t'.join(f'{v:08x}' for v in row) + '\n')
    print(f'{cases} cases written to {OUT}/cases.tsv')


if __name__ == '__main__':
    main()
