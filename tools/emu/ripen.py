"""An orchard tree's fruit ripening, from the original executable.

Usage: .venv/bin/python tools/emu/ripen.py [CASES]

Runs FUN_14004a740 (FruitComponent's scheduled update) for CASES chosen trees (default 2000): the
FruitDescription at +0x18 (IsDerivedFrom stubbed; months +0x20, best temperature +0x24, range
+0x28, some ranges 0), the tree at +0x28 (its growth +0x28, some above 1 and some NaN), the
ripeness +0x30, picked +0x34, the calendar's time into the year (Game +0x128 -> +8), the
temperature (Game +0x130 -> +0x18, some at the best) and the elapsed time. Writes
game-data/derived/emu/ripen/cases.tsv, floats as bits in hex: months, best, range, growth,
ripeness, picked, the calendar time, the temperature, the elapsed time, then the ripeness and
picked after.
"""
import os
import random
import struct
import sys

sys.path.insert(0, os.path.dirname(__file__))
from pe_emu import Emu  # noqa: E402

OUT = 'game-data/derived/emu/ripen'


def f32(v):
    return struct.unpack('<f', struct.pack('<f', v))[0]


def bits(v):
    return struct.unpack('<I', struct.pack('<f', v))[0]


def main():
    cases = int(sys.argv[1]) if len(sys.argv) > 1 else 2000
    e = Emu('game-data/original/Application-steam-x64.exe', 'game-data/original/Runtime-steam-x64.dll')
    e.hook_function(0x1800cd0d0, 'platform counter setup', lambda a: 1)
    e.call(0x1800cd030)
    e.hook_function(e.exports['?IsDerivedFrom@Base@Object@@QEBA_NI@Z'], 'synthetic type', lambda a: 1)
    game, calendar, env, fruit, desc, tree, dt = [e.malloc(n) for n in (0x160, 0x40, 0x40, 0x40, 0x40, 0x40, 8)]
    e.write(0x1401cbdd8, struct.pack('<Q', game))
    e.write(game + 0x128, struct.pack('<Q', calendar))
    e.write(game + 0x130, struct.pack('<Q', env))
    e.write(fruit + 0x18, struct.pack('<Q', desc))
    e.write(fruit + 0x28, struct.pack('<Q', tree))
    r = random.Random(0x14004a740)
    os.makedirs(OUT, exist_ok=True)
    with open(f'{OUT}/cases.tsv', 'w') as out:
        out.write('months\tbest\trange\tgrowth\tripeness\tpicked\ttime\ttemperature\tdt\tripeness_after\tpicked_after\n')
        for k in range(cases):
            months, best = f32(r.uniform(0.5, 4)), f32(r.uniform(40, 70))
            rng_ = f32(0.0 if k % 25 == 0 else r.uniform(5, 30))
            growth = f32(r.choice([r.uniform(0, 1), 1.0, r.uniform(1, 2), float('nan') if k % 40 == 1 else 1.0]))
            ripeness = f32(r.choice([r.uniform(0, 1), 0.0, 0.999]))
            picked = r.random() < 0.3
            time = r.randrange(0, 3600 * 1_000_000)
            temp = f32(best if k % 25 == 0 else r.choice([r.uniform(0, 100), r.uniform(20, 32)]))
            elapsed = r.choice([16666, 166660, r.randrange(0, 60_000_000)])
            e.write(desc + 0x20, struct.pack('<fff', months, best, rng_))
            e.write(tree + 0x28, struct.pack('<f', growth))
            e.write(fruit + 0x30, struct.pack('<f', ripeness))
            e.write(fruit + 0x34, bytes([int(picked)]))
            e.write(calendar + 8, struct.pack('<q', time))
            e.write(env + 0x18, struct.pack('<f', temp))
            e.write(dt, struct.pack('<q', elapsed))
            e.call(0x14004a740, fruit, dt)
            after = struct.unpack('<I', e.read(fruit + 0x30, 4))[0]
            picked_after = e.read(fruit + 0x34, 1)[0]
            row = [f'{bits(months):x}', f'{bits(best):x}', f'{bits(rng_):x}', f'{bits(growth):x}',
                   f'{bits(ripeness):x}', int(picked), time, f'{bits(temp):x}', elapsed, f'{after:x}', picked_after]
            out.write('\t'.join(map(str, row)) + '\n')
    print(f'{cases} cases written to {OUT}/cases.tsv')


if __name__ == '__main__':
    main()
