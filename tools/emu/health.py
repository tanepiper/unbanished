"""A citizen's health after an interval's events, from the original executable.

Usage: .venv/bin/python tools/emu/health.py [CASES]

Runs FUN_140117e60 for CASES chosen citizens (default 2000): the events at +0x54 (any of the bits
0x001 to 0x200), the health at +0x50 (some negative, some above the maximum, some NaN), the age
at +0x3c (some NaN), a school at +0x208 or none, and the description's adult age +0x44 and maximum
+0x70 (Game +0xd0 bit 3, the debug flag, clear). Writes game-data/derived/emu/health/cases.tsv,
floats as bits in hex: events, health, age, school, adult age, maximum, then the health after
and the events after.
"""
import os
import random
import struct
import sys

sys.path.insert(0, os.path.dirname(__file__))
from pe_emu import Emu  # noqa: E402

OUT = 'game-data/derived/emu/health'


def bits(v):
    return struct.unpack('<I', struct.pack('<f', v))[0]


def main():
    cases = int(sys.argv[1]) if len(sys.argv) > 1 else 2000
    e = Emu('game-data/original/Application-steam-x64.exe', 'game-data/original/Runtime-steam-x64.dll')
    e.hook_function(0x1800cd0d0, 'platform counter setup', lambda a: 1)
    e.call(0x1800cd030)
    game, citizen, desc, school = [e.malloc(n) for n in (0x160, 0x300, 0x100, 0x40)]
    e.write(0x1401cbdd8, struct.pack('<Q', game))
    e.write(game + 0xd0, struct.pack('<I', 0))
    e.write(citizen + 0x288, struct.pack('<Q', desc))
    r = random.Random(0x140117e60)
    os.makedirs(OUT, exist_ok=True)
    with open(f'{OUT}/cases.tsv', 'w') as out:
        out.write('events\thealth\tage\tschool\tadult_age\tmax\thealth_after\tevents_after\n')
        for k in range(cases):
            events = r.randrange(0, 0x400)
            health = r.choice([r.uniform(0, 10), r.uniform(-3, 0), r.uniform(10, 15), 0.0, -0.0,
                               float('nan') if k % 40 == 0 else 5.0])
            age = r.choice([r.uniform(0, 20), 4.0, float('nan') if k % 30 == 0 else 8.0])
            at_school = r.random() < 0.3
            adult, mx = r.uniform(2, 6), r.choice([10.0, r.uniform(5, 12)])
            vals = [struct.unpack('<f', struct.pack('<f', v))[0] for v in (health, age, adult, mx)]
            health, age, adult, mx = vals
            e.write(citizen + 0x54, struct.pack('<I', events))
            e.write(citizen + 0x50, struct.pack('<f', health))
            e.write(citizen + 0x3c, struct.pack('<f', age))
            e.write(citizen + 0x208, struct.pack('<Q', school if at_school else 0))
            e.write(desc + 0x44, struct.pack('<f', adult))
            e.write(desc + 0x70, struct.pack('<f', mx))
            e.call(0x140117e60, citizen)
            after = struct.unpack('<I', e.read(citizen + 0x50, 4))[0]
            events_after = struct.unpack('<I', e.read(citizen + 0x54, 4))[0]
            row = [events, bits(health), bits(age), int(at_school), bits(adult), bits(mx), after, events_after]
            out.write('\t'.join(f'{v:x}' for v in row) + '\n')
    print(f'{cases} cases written to {OUT}/cases.tsv')


if __name__ == '__main__':
    main()
