"""Ground truth for the C runtime maths the game's code calls (crates/core/src/crt.rs): `sinf`
(`FUN_140137160`), `cosf` (`FUN_140136560`), `acosf` (`FUN_140136090`) and the Runtime's
`atan2f` (`0x180128f10`).

Usage: .venv/bin/python tools/emu/crt_math.py OUT_DIR [COUNT]

Writes `OUT_DIR/crt_math.tsv`: `name<TAB>argument bits<TAB>result bits` (hex; two arguments for
atan2f, y then x), COUNT random arguments per function (default 200000) over wide ranges, plus
edge values (zeros, signs, ±1, tiny, the reduction boundaries). Arguments that reach the C
runtime's error handler (infinities, NaNs, acosf outside [-1, 1]) are skipped: the emulator
cannot raise, and the game never passes them.
"""
import os
import random
import struct
import sys

sys.path.insert(0, 'tools/emu')
from pe_emu import Emu, F32  # noqa: E402

FUNCS = {'sinf': 0x140137160, 'cosf': 0x140136560, 'acosf': 0x140136090, 'atan2f': 0x180128f10}


def f32(x):
    return struct.unpack('<f', struct.pack('<f', x))[0]


def bits(x):
    return '%08x' % struct.unpack('<I', struct.pack('<f', x))[0]


def main(out_dir, count=200000):
    os.makedirs(out_dir, exist_ok=True)
    emu = Emu('game-data/original/Application-steam-x64.exe',
              'game-data/original/Runtime-steam-x64.dll')
    rng = random.Random(20261017)
    inf = float('inf')
    edges = [0.0, -0.0, 1.0, -1.0, 0.5, -0.5, 1e-30, -1e-30, 2 ** -10, 2 ** -7, 0.7853981,
             0.7853982, 1.5707963, 3.1415927, -3.1415927, 6.2831855, 100.0, 1000.0, 499999.0,
             ]
    rows = []

    def call(addr, *args):
        try:
            return emu.call_f32(addr, *args)
        except RuntimeError:
            return None

    def angle():
        k = rng.random()
        if k < 0.6:
            return f32(rng.uniform(-7, 7))
        if k < 0.8:
            return f32(rng.uniform(-1000, 1000))
        if k < 0.9:
            return f32(rng.choice([1, -1]) * 10 ** rng.uniform(-9, 0))
        return f32(rng.uniform(-400000, 400000))

    for name in ('sinf', 'cosf'):
        for x in edges + [angle() for _ in range(count)]:
            x = f32(x)
            r = call(FUNCS[name], F32(x))
            if r is not None:
                rows.append(f'{name}\t{bits(x)}\t{bits(r)}')
    for x in [0.0, -0.0, 1.0, -1.0, 0.5, -0.5, 0.4999999, 1e-9, -1e-9] + [
            f32(rng.uniform(-1, 1) if rng.random() < 0.9 else rng.choice([1, -1]) * 10 ** rng.uniform(-9, 0))
            for _ in range(count)]:
        x = f32(x)
        r = call(FUNCS['acosf'], F32(x))
        if r is not None:
            rows.append(f'acosf\t{bits(x)}\t{bits(r)}')
    pairs = [(0.0, 1.0), (0.0, -1.0), (-0.0, -1.0), (1.0, 0.0), (-1.0, 0.0), (inf, inf), (-inf, inf),
             (inf, -inf), (1e-30, 1.0), (1e-30, -1.0), (1.0, 1e-30), (1e-40, 1e5)]
    for _ in range(count):
        k = rng.random()
        if k < 0.6:
            y, x = rng.uniform(-60, 60), rng.uniform(-60, 60)
        elif k < 0.85:
            x = rng.uniform(-60, 60)
            y = x * rng.choice([1, -1]) * 10 ** rng.uniform(-9, 1)
        else:
            y = rng.choice([1, -1]) * 10 ** rng.uniform(-40, 30)
            x = rng.choice([1, -1]) * 10 ** rng.uniform(-40, 30)
        pairs.append((y, x))
    for y, x in pairs:
        y, x = f32(y), f32(x)
        r = call(FUNCS['atan2f'], F32(y), F32(x))
        if r is None:
            continue
        rows.append(f'atan2f\t{bits(y)},{bits(x)}\t{bits(r)}')
    with open(os.path.join(out_dir, 'crt_math.tsv'), 'w') as f:
        f.write('\n'.join(rows) + '\n')
    print(f'wrote {len(rows)} results')


if __name__ == '__main__':
    main(sys.argv[1], *(int(a) for a in sys.argv[2:]))
