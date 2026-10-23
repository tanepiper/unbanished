"""The renderer's seasonal values, from the original executable.

Usage: .venv/bin/python tools/emu/season.py [CASES]

Runs FUN_1400988f0 for CASES chosen environments (default 2000): the temperature at +0x18 (around
32 °F, far from it, and some NaN), the calendar's time into the year (Game +0x128 -> +8, all
months and the edges of 0.5, 6, 7.5 months), and 0 to 2 weather effects (+0x50, count +0x58;
some slots empty) with their intensities at +0xc0. Writes game-data/derived/emu/season/cases.tsv,
floats as bits in hex: temperature, the time, the intensities (empty slots as -), then snow,
autumn, leaf loss and rain.
"""
import os
import random
import struct
import sys

sys.path.insert(0, os.path.dirname(__file__))
from pe_emu import Emu  # noqa: E402

OUT = 'game-data/derived/emu/season'


def f32(v):
    return struct.unpack('<f', struct.pack('<f', v))[0]


def bits(v):
    return struct.unpack('<I', struct.pack('<f', v))[0]


def main():
    cases = int(sys.argv[1]) if len(sys.argv) > 1 else 2000
    e = Emu('game-data/original/Application-steam-x64.exe', 'game-data/original/Runtime-steam-x64.dll')
    e.hook_function(0x1800cd0d0, 'platform counter setup', lambda a: 1)
    e.call(0x1800cd030)
    game, calendar, env, slots = [e.malloc(n) for n in (0x160, 0x40, 0x80, 16)]
    effects = [e.malloc(0xd0) for _ in range(2)]
    e.write(0x1401cbdd8, struct.pack('<Q', game))
    e.write(game + 0x128, struct.pack('<Q', calendar))
    e.write(env + 0x50, struct.pack('<Q', slots))
    r = random.Random(0x1400988f0)
    month = 300 * 1_000_000
    os.makedirs(OUT, exist_ok=True)
    with open(f'{OUT}/cases.tsv', 'w') as out:
        out.write('temperature\ttime\tintensities\tsnow\tautumn\tleaf_loss\train\n')
        for k in range(cases):
            temp = f32(r.choice([r.uniform(25, 40), r.uniform(-20, 100), 32.0, 31.0, 33.0,
                                 float('nan') if k % 40 == 0 else 50.0]))
            time = r.choice([r.randrange(0, 12 * month), month // 2, 6 * month, int(7.5 * month),
                             month // 2 - 1, 6 * month - 1, int(7.5 * month) + 1, 0])
            n = r.randrange(0, 3)
            items = []
            for i in range(n):
                if r.random() < 0.2:
                    items.append(None)
                    e.write(slots + 8 * i, struct.pack('<Q', 0))
                else:
                    v = f32(r.uniform(0, 1))
                    items.append(v)
                    e.write(effects[i] + 0xc0, struct.pack('<f', v))
                    e.write(slots + 8 * i, struct.pack('<Q', effects[i]))
            e.write(env + 0x18, struct.pack('<f', temp))
            e.write(env + 0x58, struct.pack('<I', n))
            e.write(calendar + 8, struct.pack('<q', time))
            e.call(0x1400988f0, env)
            vals = struct.unpack('<4I', e.read(env + 0x1c, 16))
            inten = ','.join('-' if v is None else f'{bits(v):x}' for v in items)
            out.write('\t'.join([f'{bits(temp):x}', str(time), inten] + [f'{v:x}' for v in vals]) + '\n')
    print(f'{cases} cases written to {OUT}/cases.tsv')


if __name__ == '__main__':
    main()
