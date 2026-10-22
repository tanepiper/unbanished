"""The time to a trading post's next merchant, from the original executable.

Usage: .venv/bin/python tools/emu/trade_timer.py [CASES]

For CASES chosen trade descriptions and populations (default 2000), runs FUN_14005be50 (the row
for the town's population) and FUN_14005bdb0 (the time to the next merchant), and writes
game-data/derived/emu/trade_timer/cases.tsv: one line per case with the seed, the population
(Game +0xf0 -> +0xa8), the rows (population,months,range;...: TradeDescription +0x58, count
+0x60, 0x14 bytes each), the row chosen, the time written to TradeComponent +0x78, and whether the
game's Random moved. IsDerivedFrom is hooked to say yes: the description is synthetic. Some rows
are out of order, and some populations are below 0.
"""
import os
import random
import struct
import sys

sys.path.insert(0, os.path.dirname(__file__))
from pe_emu import Emu  # noqa: E402

OUT = 'game-data/derived/emu/trade_timer'


def f32(v):
    return struct.unpack('<f', struct.pack('<f', v))[0]


def main():
    cases = int(sys.argv[1]) if len(sys.argv) > 1 else 2000
    e = Emu('game-data/original/Application-steam-x64.exe', 'game-data/original/Runtime-steam-x64.dll')
    e.hook_function(0x1800cd0d0, 'platform counter setup', lambda a: 1)
    e.call(0x1800cd030)
    e.hook_function(e.exports['?IsDerivedFrom@Base@Object@@QEBA_NI@Z'], 'synthetic type', lambda a: 1)
    game, town, rng, component, desc, rows = [e.malloc(n) for n in (0x160, 0x200, 2504, 0x100, 0x100, 0x14 * 8)]
    e.write(0x1401cbdd8, struct.pack('<Q', game))
    e.write(game + 0xd8, struct.pack('<Q', rng))
    e.write(game + 0xf0, struct.pack('<Q', town))
    e.write(component + 0x18, struct.pack('<Q', desc))
    e.write(desc + 0x58, struct.pack('<Q', rows))
    r = random.Random(0x14005bdb0)
    os.makedirs(OUT, exist_ok=True)
    with open(f'{OUT}/cases.tsv', 'w') as out:
        out.write('seed\tpopulation\trows\trow\ttimer\tdrew\n')
        for k in range(cases):
            seed = r.randrange(1, 1 << 31)
            n = r.randrange(1, 7)
            pops = sorted(r.randrange(0, 400) for _ in range(n))
            if k % 10 == 0:
                r.shuffle(pops)
            table = [(p, f32(r.uniform(0.0, 12.0)), f32(r.uniform(0.0, 6.0))) for p in pops]
            population = r.choice([r.randrange(0, 450), -1, 0, pops[-1], pops[0]])
            for i, (p, months, rng_range) in enumerate(table):
                e.write(rows + 0x14 * i, struct.pack('<iff', p, months, rng_range))
            e.write(desc + 0x60, struct.pack('<i', n))
            e.write(town + 0xa8, struct.pack('<i', population))
            e.call(e.exports['??0Random@System@@QEAA@I@Z'], rng, seed)
            row = e.call_i32(0x14005be50, component)
            before = e.read(rng, 2504)
            e.call(0x14005bdb0, component)
            drew = e.read(rng, 2504) != before
            timer = struct.unpack('<q', e.read(component + 0x78, 8))[0]
            text = ';'.join(f'{p},{m!r},{g!r}' for p, m, g in table)
            out.write(f'{seed}\t{population}\t{text}\t{row}\t{timer}\t{int(drew)}\n')
    print(f'{cases} cases written to {OUT}/cases.tsv')


if __name__ == '__main__':
    main()
