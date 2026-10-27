"""The end of a citizen's illness, from the original executable.

Usage: .venv/bin/python tools/emu/recovery.py [CASES]

Runs FUN_140117880 (recover or die) on a sick citizen for CASES chosen inputs (default 2000), and
writes game-data/derived/emu/recovery/cases.tsv: one line per case with the inputs (seed, the
disease's recovery +0x40, whether a doctor is set at +0x228, the description's doctor factor
+0x128 and immunity +0x9c) and the outputs (the return value, the immunity +0x70 and spread timer
+0x78 after, whether the disease +0x68 was cleared, and whether the game's Random moved). Leaving
the doctor (FUN_14011ae80) is hooked: it only records the call. Some cases use a recovery or a
doctor factor of 0 or less, or NaN.
"""
import os
import random
import struct
import sys

sys.path.insert(0, os.path.dirname(__file__))
from pe_emu import Emu  # noqa: E402

OUT = 'game-data/derived/emu/recovery'


def f32(v):
    return struct.unpack('<f', struct.pack('<f', v))[0]


def main():
    cases = int(sys.argv[1]) if len(sys.argv) > 1 else 2000
    e = Emu('game-data/original/Application-steam-x64.exe', 'game-data/original/Runtime-steam-x64.dll')
    e.hook_function(0x1800cd0d0, 'platform counter setup', lambda a: 1)
    e.call(0x1800cd030)
    left = []
    e.hook_function(0x14011ae80, 'leave the doctor', lambda a: left.append(a[0]) or 0)
    game, rng, citizen, desc, disease, doctor = [e.malloc(n) for n in (0x160, 2504, 0x300, 0x200, 0x100, 0x40)]
    e.write(0x1401cbdd8, struct.pack('<Q', game))
    e.write(game + 0xd8, struct.pack('<Q', rng))
    e.write(citizen + 0x288, struct.pack('<Q', desc))
    r = random.Random(140117880)
    os.makedirs(OUT, exist_ok=True)
    with open(f'{OUT}/cases.tsv', 'w') as out:
        out.write('seed\trecovery\tdoctor\tfactor\timmunity_minutes\t'
                  'returned\timmunity_after\tspread_after\tcleared\tdrew\tleft\n')
        for k in range(cases):
            seed = r.randrange(1, 1 << 31)
            recovery = f32(r.choice([r.uniform(0.5, 20.0), r.uniform(1.0, 3.0), 1.0]))
            if k % 20 == 0:
                recovery = f32(r.choice([0.0, -r.uniform(0.1, 2.0), float('nan')]))
            has_doctor = r.random() < 0.5
            factor = f32(r.choice([r.uniform(0.5, 4.0), 1.0, 2.0]))
            if k % 25 == 0:
                factor = f32(r.choice([0.0, -1.0, float('nan')]))
            immunity = f32(r.uniform(0.0, 30.0))
            e.call(e.exports['??0Random@System@@QEAA@I@Z'], rng, seed)
            e.write(disease + 0x40, struct.pack('<f', recovery))
            e.write(citizen + 0x68, struct.pack('<Q', disease))
            e.write(citizen + 0x70, struct.pack('<q', 12345))
            e.write(citizen + 0x78, struct.pack('<q', 67890))
            e.write(citizen + 0x228, struct.pack('<Q', doctor if has_doctor else 0))
            e.write(desc + 0x128, struct.pack('<f', factor))
            e.write(desc + 0x9c, struct.pack('<f', immunity))
            before = e.read(rng, 2504)
            left.clear()
            returned = e.call(0x140117880, citizen) & 0xff
            drew = e.read(rng, 2504) != before
            immunity_after, spread_after = struct.unpack('<qq', e.read(citizen + 0x70, 16))
            cleared = struct.unpack('<Q', e.read(citizen + 0x68, 8))[0] == 0
            out.write('\t'.join(str(x) for x in [seed, repr(recovery), int(has_doctor), repr(factor), repr(immunity),
                                                 returned, immunity_after, spread_after, int(cleared), int(drew),
                                                 len(left)]) + '\n')
    print(f'{cases} cases written to {OUT}/cases.tsv')


if __name__ == '__main__':
    main()
