"""The chance of an event that happens once in a span of years, from the original executable.

Usage: .venv/bin/python tools/emu/once_in_years.py [CASES]

Runs FUN_1400ae760(Time *timer, float pair[2], Random *rng) (injuries at work, tornadoes, fires
that start by themselves) on CASES chosen inputs (default 2000) and writes
game-data/derived/emu/once_in_years/cases.tsv: the seed, the clock, the two floats (years, odds),
then the clock after, whether the Random moved and the result. About a tenth of the cases have
odds of 0 or less.
"""
import os
import random
import struct
import sys

sys.path.insert(0, os.path.dirname(__file__))
from pe_emu import Emu  # noqa: E402

OUT = 'game-data/derived/emu/once_in_years'


def f32(x):
    return struct.unpack('<f', struct.pack('<f', x))[0]


def main():
    cases = int(sys.argv[1]) if len(sys.argv) > 1 else 2000
    e = Emu('game-data/original/Application-steam-x64.exe', 'game-data/original/Runtime-steam-x64.dll')
    e.hook_function(0x1800cd0d0, 'platform counter setup', lambda a: 1)
    e.call(0x1800cd030)
    rng, timer, pair = e.malloc(2504), e.malloc(8), e.malloc(8)
    r = random.Random(1400)
    os.makedirs(OUT, exist_ok=True)
    with open(f'{OUT}/cases.tsv', 'w') as out:
        out.write('seed\ttimer\tyears\todds\ttimer_after\tdrew\thappens\n')
        for k in range(cases):
            seed = r.randrange(1, 1 << 31)
            clock = r.choice([0, r.randrange(0, 50_000_000), r.randrange(0, 40_000_000_000)])
            years = f32(r.choice([r.uniform(0.01, 2.0), r.uniform(1.0, 100.0)]))
            odds = f32(r.uniform(0.01, 3.0))
            if k % 10 == 0:
                odds = f32(r.choice([0.0, -r.uniform(0.01, 2.0)]))
            e.call(e.exports['??0Random@System@@QEAA@I@Z'], rng, seed)
            e.write(timer, struct.pack('<q', clock))
            e.write(pair, struct.pack('<ff', years, odds))
            before = e.read(rng, 2504)
            happens = e.call(0x1400ae760, timer, pair, rng) & 0xff
            drew = e.read(rng, 2504) != before
            after = struct.unpack('<q', e.read(timer, 8))[0]
            out.write(f'{seed}\t{clock}\t{years!r}\t{odds!r}\t{after}\t{int(drew)}\t{int(bool(happens))}\n')
    print(f'{cases} cases written to {OUT}/cases.tsv')


if __name__ == '__main__':
    main()
