"""The disease check of a citizen with no disease, from the original executable.

Usage: .venv/bin/python tools/emu/disease_check.py [CASES]

Runs FUN_140117ca0 (CitizenComponent's disease update) on a citizen with no disease (+0x68 null)
for CASES chosen inputs (default 2000), and writes game-data/derived/emu/disease/cases.tsv: one
line per case with the inputs (seed, dt, health +0x50, immunity +0x70, clock +0x60, the town's
disease +0x1a8 of Game +0xf0, and the description's floats +0x70 and +0x7c to +0x90) and the
outputs (immunity and clock after, whether the game's Random moved, whether the outbreak
FUN_140117af0 was called, and the return value). The outbreak is hooked: it only records the call.
About a tenth of the cases have odds of 0 or less, where the original returns before its draw.
"""
import os
import random
import struct
import sys

sys.path.insert(0, os.path.dirname(__file__))
from pe_emu import Emu  # noqa: E402

OUT = 'game-data/derived/emu/disease'
DESC = [0x70, 0x7c, 0x80, 0x84, 0x88, 0x8c, 0x90]


def main():
    cases = int(sys.argv[1]) if len(sys.argv) > 1 else 2000
    e = Emu('game-data/original/Application-steam-x64.exe', 'game-data/original/Runtime-steam-x64.dll')
    e.hook_function(0x1800cd0d0, 'platform counter setup', lambda a: 1)
    e.call(0x1800cd030)
    outbreaks = []
    e.hook_function(0x140117af0, 'outbreak', lambda a: outbreaks.append(a[0]) or 0)
    game, town, rng, citizen, desc, dt_ptr = [e.malloc(n) for n in (0x160, 0x200, 2504, 0x300, 0x100, 8)]
    e.write(0x1401cbdd8, struct.pack('<Q', game))
    e.write(game + 0xd8, struct.pack('<Q', rng))
    e.write(game + 0xf0, struct.pack('<Q', town))
    e.write(citizen + 0x288, struct.pack('<Q', desc))
    r = random.Random(140117)
    os.makedirs(OUT, exist_ok=True)
    with open(f'{OUT}/cases.tsv', 'w') as out:
        out.write('seed\tdt\thealth\timmunity\tclock\ttown\t' + '\t'.join(f'd{o:x}' for o in DESC)
                  + '\timmunity_after\tclock_after\tdrew\toutbreak\treturned\n')
        for k in range(cases):
            seed = r.randrange(1, 1 << 31)
            dt = r.choice([0, 16666, 166660, r.randrange(1, 10_000_000)])
            health = r.uniform(0.0, 10.0)
            immunity = r.choice([0, r.randrange(0, 300_000_000), -5])
            clock = r.randrange(0, 4_000_000_000)
            town_disease = r.choice([0.0, r.random(), 1.0])
            d = {0x70: r.uniform(1.0, 10.0)}
            for o in DESC[1:]:
                d[o] = r.uniform(0.05, 4.0)
            if k % 10 == 0:
                # Odds of 0 or less: the second value at every weight.
                for o in (0x80, 0x88, 0x90):
                    d[o] = r.choice([0.0, -r.uniform(0.01, 2.0)])
            e.call(e.exports['??0Random@System@@QEAA@I@Z'], rng, seed)
            e.write(citizen + 0x68, struct.pack('<Q', 0))
            e.write(citizen + 0x50, struct.pack('<f', health))
            e.write(citizen + 0x70, struct.pack('<q', immunity))
            e.write(citizen + 0x60, struct.pack('<q', clock))
            e.write(town + 0x1a8, struct.pack('<f', town_disease))
            for o, v in d.items():
                e.write(desc + o, struct.pack('<f', v))
            e.write(dt_ptr, struct.pack('<q', dt))
            # The inputs as the original holds them (f32), for the port's test.
            health, town_disease = struct.unpack('<ff', struct.pack('<ff', health, town_disease))
            d = {o: struct.unpack('<f', struct.pack('<f', v))[0] for o, v in d.items()}
            before = e.read(rng, 2504)
            outbreaks.clear()
            returned = e.call(0x140117ca0, citizen, dt_ptr) & 0xff
            drew = e.read(rng, 2504) != before
            immunity_after = struct.unpack('<q', e.read(citizen + 0x70, 8))[0]
            clock_after = struct.unpack('<q', e.read(citizen + 0x60, 8))[0]
            out.write('\t'.join(str(x) for x in [seed, dt, repr(health), immunity, clock, repr(town_disease)]
                                + [repr(d[o]) for o in DESC]
                                + [immunity_after, clock_after, int(drew), int(bool(outbreaks)), returned]) + '\n')
    print(f'{cases} cases written to {OUT}/cases.tsv')


if __name__ == '__main__':
    main()
