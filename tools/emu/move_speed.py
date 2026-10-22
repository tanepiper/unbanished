"""The speed factor of a MoveTo action, from the original executable.

Usage: .venv/bin/python tools/emu/move_speed.py [CASES]

Runs FUN_1400ce640 (the MoveTo constructor) for CASES chosen speeds
(default 2000) and writes game-data/derived/emu/move_speed/cases.tsv: one line per case with the
seed, the speed passed in (as f32 bits, so -0 and NaN survive), the speed factor at +0x478 after
(bits) and whether the game's Random moved. IsDerivedFrom is hooked to say yes (the owner's
description is synthetic), and the object's identity registration is skipped. Speeds below 0 and NaN draw a factor; 0 or more is kept.
"""
import os
import random
import struct
import sys

sys.path.insert(0, os.path.dirname(__file__))
from pe_emu import Emu, F32  # noqa: E402

OUT = 'game-data/derived/emu/move_speed'


def bits(v):
    return struct.unpack('<I', struct.pack('<f', v))[0]


def main():
    cases = int(sys.argv[1]) if len(sys.argv) > 1 else 2000
    e = Emu('game-data/original/Application-steam-x64.exe', 'game-data/original/Runtime-steam-x64.dll')
    e.hook_function(0x1800cd0d0, 'platform counter setup', lambda a: 1)
    e.call(0x1800cd030)
    e.hook_function(e.exports['?IsDerivedFrom@Base@Object@@QEBA_NI@Z'], 'synthetic type', lambda a: 1)
    # The object's identity comes from the object factory's own Random, never the game's.
    e.hook_function(e.exports['??0Internal@Object@@QEAA@XZ'], 'object identity', lambda a: a[0])
    game, rng, action, owner, owner_ptr, desc, target = [
        e.malloc(n) for n in (0x160, 2504, 0x500, 0x40, 8, 0x100, 12)]
    e.write(0x1401cbdd8, struct.pack('<Q', game))
    e.write(game + 0xd8, struct.pack('<Q', rng))
    e.write(owner + 0x18, struct.pack('<Q', desc))
    e.write(owner_ptr, struct.pack('<Q', owner))
    e.write(target, struct.pack('<fff', 1.0, 2.0, 3.0))
    r = random.Random(0x1400ce640)
    os.makedirs(OUT, exist_ok=True)
    with open(f'{OUT}/cases.tsv', 'w') as out:
        out.write('seed\tspeed_bits\tfactor_bits\tdrew\n')
        for k in range(cases):
            seed = r.randrange(1, 1 << 31)
            speed = r.choice([-1.0, -r.uniform(0.0, 5.0), r.uniform(0.0, 5.0), 0.0, -0.0, 1.0,
                              float('nan'), -1e-30])
            e.call(e.exports['??0Random@System@@QEAA@I@Z'], rng, seed)
            before = e.read(rng, 2504)
            e.call(0x1400ce640, action, owner_ptr, 3, 4, target, 5, 6, F32(speed))
            drew = e.read(rng, 2504) != before
            factor = struct.unpack('<I', e.read(action + 0x478, 4))[0]
            out.write(f'{seed}\t{bits(speed)}\t{factor}\t{int(drew)}\n')
    print(f'{cases} cases written to {OUT}/cases.tsv')


if __name__ == '__main__':
    main()
