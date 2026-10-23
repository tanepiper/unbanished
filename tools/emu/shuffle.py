"""A list shuffled with the game's Random, from the original executable.

Usage: .venv/bin/python tools/emu/shuffle.py [CASES]

Runs FUN_14008eb70 (the residences' shuffle; the workplaces' FUN_14008e940 does the same once it
has found the profession's list) on CASES lists of 0 to 40 entries (default 2000), and writes
game-data/derived/emu/shuffle/cases.tsv: the seed, the list's length (+0x28), the order after
(each entry was its index; the array is at +0x20) and the next Rand of the game's Random, which
shows how many numbers were drawn.
"""
import os
import random
import struct
import sys

sys.path.insert(0, os.path.dirname(__file__))
from pe_emu import Emu  # noqa: E402

OUT = 'game-data/derived/emu/shuffle'


def main():
    cases = int(sys.argv[1]) if len(sys.argv) > 1 else 2000
    e = Emu('game-data/original/Application-steam-x64.exe', 'game-data/original/Runtime-steam-x64.dll')
    e.hook_function(0x1800cd0d0, 'platform counter setup', lambda a: 1)
    e.call(0x1800cd030)
    game, rng, holder, array = [e.malloc(n) for n in (0x160, 2504, 0x40, 8 * 64)]
    e.write(0x1401cbdd8, struct.pack('<Q', game))
    e.write(game + 0xd8, struct.pack('<Q', rng))
    e.write(holder + 0x20, struct.pack('<Q', array))
    r = random.Random(0x14008eb70)
    os.makedirs(OUT, exist_ok=True)
    with open(f'{OUT}/cases.tsv', 'w') as out:
        out.write('seed\tn\torder\tnext\n')
        for _ in range(cases):
            seed = r.randrange(1, 1 << 31)
            n = r.choice([0, 1, 2, 3, r.randrange(0, 41)])
            e.write(array, b''.join(struct.pack('<Q', i) for i in range(n)))
            e.write(holder + 0x28, struct.pack('<i', n))
            e.call(e.exports['??0Random@System@@QEAA@I@Z'], rng, seed)
            e.call(0x14008eb70, holder)
            order = [struct.unpack('<Q', e.read(array + 8 * i, 8))[0] for i in range(n)]
            after = e.call(e.exports['?Rand@Random@System@@QEAAIXZ'], rng) & 0xffffffff
            out.write(f"{seed}\t{n}\t{','.join(map(str, order))}\t{after}\n")
    print(f'{cases} cases written to {OUT}/cases.tsv')


if __name__ == '__main__':
    main()
