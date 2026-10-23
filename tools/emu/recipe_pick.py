"""A production cycle's recipe, chosen by weight, from the original executable.

Usage: .venv/bin/python tools/emu/recipe_pick.py [CASES]

Runs FUN_14011e940 on CASES chosen ConsumeProduce states (default 2000): the selector at +0x50
(mostly -1, any recipe; some 0 or more, some below -1) and 0 to 6 weights at +0x58 (count +0x60),
some 0 or negative. Writes game-data/derived/emu/recipe_pick/cases.tsv: the seed, the selector,
the weights, the recipe returned and the next Rand of the game's Random, which shows whether a
number was drawn.
"""
import os
import random
import struct
import sys

sys.path.insert(0, os.path.dirname(__file__))
from pe_emu import Emu  # noqa: E402

OUT = 'game-data/derived/emu/recipe_pick'


def f32(v):
    return struct.unpack('<f', struct.pack('<f', v))[0]


def main():
    cases = int(sys.argv[1]) if len(sys.argv) > 1 else 2000
    e = Emu('game-data/original/Application-steam-x64.exe', 'game-data/original/Runtime-steam-x64.dll')
    e.hook_function(0x1800cd0d0, 'platform counter setup', lambda a: 1)
    e.call(0x1800cd030)
    game, rng, state, weights = [e.malloc(n) for n in (0x160, 2504, 0x80, 4 * 8)]
    e.write(0x1401cbdd8, struct.pack('<Q', game))
    e.write(game + 0xd8, struct.pack('<Q', rng))
    e.write(state + 0x58, struct.pack('<Q', weights))
    r = random.Random(0x14011e940)
    os.makedirs(OUT, exist_ok=True)
    with open(f'{OUT}/cases.tsv', 'w') as out:
        out.write('seed\tselector\tweights\tpicked\tnext\n')
        for k in range(cases):
            seed = r.randrange(1, 1 << 31)
            selector = -1 if k % 10 < 8 else r.choice([0, 1, 3, -2, -5])
            n = r.randrange(0, 7)
            w = [f32(r.choice([r.uniform(0.0, 5.0), 1.0, 0.0, -r.uniform(0.0, 1.0)])) for _ in range(n)]
            e.write(weights, struct.pack(f'<{n}f', *w))
            e.write(state + 0x50, struct.pack('<i', selector))
            e.write(state + 0x60, struct.pack('<i', n))
            e.call(e.exports['??0Random@System@@QEAA@I@Z'], rng, seed)
            picked = e.call_i32(0x14011e940, state)
            after = e.call(e.exports['?Rand@Random@System@@QEAAIXZ'], rng) & 0xffffffff
            out.write(f"{seed}\t{selector}\t{','.join(repr(x) for x in w)}\t{picked}\t{after}\n")
    print(f'{cases} cases written to {OUT}/cases.tsv')


if __name__ == '__main__':
    main()
