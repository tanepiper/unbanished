"""Vector normalisation from the original Runtime.

Usage: .venv/bin/python tools/emu/normalize.py [CASES]

Runs the Runtime's `Vector3::Normalize` with its length (`0x180022300`) and without
(`0x1800223c0`), and `Vector2::Normalize` with (`0x18006d280`) and without (`0x18006d0d0`), on
CASES vectors (default 4000): ordinary ones, unit ones, tiny, huge (their squares overflow),
zero, -0 and denormal parts. NaN is left out: the CRT's sqrtf sends it to an error handler that
the emulator does not set up. Each
is a const member returning the vector through a hidden pointer (rcx `this`, rdx the result, r8
the length). Writes game-data/derived/emu/normalize/cases.tsv, every value as f32 bits in hex: the
input (x, y, z), then the four results (v3 with length, its length, v3, v2 with length, its
length, v2).
"""
import os
import random
import struct
import sys

sys.path.insert(0, os.path.dirname(__file__))
from pe_emu import Emu  # noqa: E402

OUT = 'game-data/derived/emu/normalize'


def bits(v):
    return struct.unpack('<I', struct.pack('<f', v))[0]


def main():
    cases = int(sys.argv[1]) if len(sys.argv) > 1 else 4000
    e = Emu('game-data/original/Application-steam-x64.exe', 'game-data/original/Runtime-steam-x64.dll')
    e.hook_function(0x1800cd0d0, 'platform counter setup', lambda a: 1)
    e.call(0x1800cd030)
    vec, out, length = [e.malloc(n) for n in (16, 16, 8)]
    r = random.Random(0x180022300)
    kinds = [lambda: r.uniform(-10, 10), lambda: r.uniform(-1, 1), lambda: r.uniform(-1e-20, 1e-20),
             lambda: r.uniform(-1e20, 1e20), lambda: 0.0, lambda: -0.0, lambda: r.uniform(-1e-40, 1e-40)]
    os.makedirs(OUT, exist_ok=True)
    rows = []
    for k in range(cases):
        kind = r.choice(kinds)
        x, y, z = kind(), kind(), kind()
        x, y, z = (struct.unpack('<f', struct.pack('<f', v))[0] for v in (x, y, z))
        e.write(vec, struct.pack('<fff', x, y, z))
        res = []
        for fn, n, with_len in ((0x180022300, 3, True), (0x1800223c0, 3, False),
                                (0x18006d280, 2, True), (0x18006d0d0, 2, False)):
            e.write(out, b'\xee' * 16)
            e.write(length, b'\xee' * 4)
            ret = e.call(fn, vec, out, length) if with_len else e.call(fn, vec, out)
            dest = ret if ret not in (0,) else out
            res += [struct.unpack('<I', e.read(dest + 4 * i, 4))[0] for i in range(n)]
            if with_len:
                res.append(struct.unpack('<I', e.read(length, 4))[0])
        rows.append([bits(x), bits(y), bits(z)] + res)
    with open(f'{OUT}/cases.tsv', 'w') as f:
        f.write('x\ty\tz\tv3l_x\tv3l_y\tv3l_z\tv3l_len\tv3_x\tv3_y\tv3_z\tv2l_x\tv2l_y\tv2l_len\tv2_x\tv2_y\n')
        for row in rows:
            f.write('\t'.join(f'{v:08x}' for v in row) + '\n')
    print(f'{cases} cases written to {OUT}/cases.tsv')


if __name__ == '__main__':
    main()
