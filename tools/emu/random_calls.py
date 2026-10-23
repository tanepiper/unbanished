"""Sequences of calls to the game's Random, from the original Runtime.

Usage: .venv/bin/python tools/emu/random_calls.py [SEEDS]

For SEEDS seeds (default 300), seeds a `System::Random` and makes 60 calls drawn at random from
`Rand()` (0x1800ca800), `Rand(unsigned)` (0x1800ca8b0: 0, 1, small, large), `Rand(int, int)`
(0x1800ca5e0: ordinary, reversed and wide ranges, never the full range, whose modulus is 0),
`RandTol(int)` (0x1800ca610: 0, small, large, negative), `RandFloat()` (0x1800ca5b0) and
`RandFloatTol()` (0x1800ca870). Writes game-data/derived/emu/random_calls/cases.tsv: one line per
call with the seed, the call's index, its kind, its two arguments and its result (a float as its
bits in hex).
"""
import os
import random
import struct
import sys

sys.path.insert(0, os.path.dirname(__file__))
from pe_emu import Emu  # noqa: E402

OUT = 'game-data/derived/emu/random_calls'
CALLS = 60


def main():
    seeds = int(sys.argv[1]) if len(sys.argv) > 1 else 300
    e = Emu('game-data/original/Application-steam-x64.exe', 'game-data/original/Runtime-steam-x64.dll')
    e.hook_function(0x1800cd0d0, 'platform counter setup', lambda a: 1)
    e.call(0x1800cd030)
    rng = e.malloc(2504)
    r = random.Random(0x1800ca800)
    os.makedirs(OUT, exist_ok=True)
    with open(f'{OUT}/cases.tsv', 'w') as out:
        out.write('seed\tcall\tkind\ta\tb\tresult\n')
        for _ in range(seeds):
            seed = r.randrange(0, 1 << 32)
            e.call(e.exports['??0Random@System@@QEAA@I@Z'], rng, seed)
            for k in range(CALLS):
                kind = r.choice(['rand', 'below', 'range', 'tol', 'float', 'floattol'])
                a = b = 0
                if kind == 'rand':
                    res = e.call(0x1800ca800, rng) & 0xffffffff
                elif kind == 'below':
                    a = r.choice([0, 1, 2, 3, r.randrange(1, 100), r.randrange(1, 1 << 32)])
                    res = e.call(0x1800ca8b0, rng, a) & 0xffffffff
                elif kind == 'range':
                    a = r.randrange(-1000, 1000)
                    b = r.choice([a + r.randrange(0, 50), a - r.randrange(1, 50), a, r.randrange(-(1 << 31), 1 << 31)])
                    if (b - a + 1) % (1 << 32) == 0:
                        b = a
                    res = e.call_i32(0x1800ca5e0, rng, a & 0xffffffff, b & 0xffffffff)
                elif kind == 'tol':
                    a = r.choice([0, 1, 2, r.randrange(1, 100), r.randrange(1, 1 << 30), -r.randrange(1, 100)])
                    res = e.call_i32(0x1800ca610, rng, a & 0xffffffff)
                elif kind == 'float':
                    res = struct.unpack('<I', struct.pack('<f', e.call_f32(0x1800ca5b0, rng)))[0]
                else:
                    res = struct.unpack('<I', struct.pack('<f', e.call_f32(0x1800ca870, rng)))[0]
                out.write(f'{seed}\t{k}\t{kind}\t{a}\t{b}\t{res}\n')
    print(f'{seeds} seeds x {CALLS} calls written to {OUT}/cases.tsv')


if __name__ == '__main__':
    main()
