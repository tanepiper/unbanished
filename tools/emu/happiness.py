"""A citizen's happiness after its events, from the original executable.

Usage: .venv/bin/python tools/emu/happiness.py [CASES]

Runs FUN_140117fb0 for CASES chosen citizens (default 2000): the flags at +0x88 (bits 0 to 18),
the happiness at +0x80 (some negative, some above the maximum, some NaN), +0x84, the food
variety at +0xa0, a priest at +0x230 or none, and a home at +0x238 or none. With a home, the
entrance cell (FUN_14008d480, stubbed) is cell 5 of a map at Game +0xf8 -> +0x80 (0x30 bytes a
cell), whose happiness layers +0x1c..+0x21 are chosen, and the residence (FUN_140008090,
stubbed) holds 1 or 2 families (+0x58). The description gives the maximum +0xa4 and the integers
+0xa8, +0xac and +0xb0. Writes game-data/derived/emu/happiness/cases.tsv: flags, happiness (bits),
+0x84, foods, priest, home, the six layers, families, maximum (bits), +0xa8, +0xac, +0xb0, then
the happiness after (bits).
"""
import os
import random
import struct
import sys

sys.path.insert(0, os.path.dirname(__file__))
from pe_emu import Emu  # noqa: E402

OUT = 'game-data/derived/emu/happiness'
CELL = 5


def bits(v):
    return struct.unpack('<I', struct.pack('<f', v))[0]


def f32(v):
    return struct.unpack('<f', struct.pack('<f', v))[0]


def main():
    cases = int(sys.argv[1]) if len(sys.argv) > 1 else 2000
    e = Emu('game-data/original/Application-steam-x64.exe', 'game-data/original/Runtime-steam-x64.dll')
    e.hook_function(0x1800cd0d0, 'platform counter setup', lambda a: 1)
    e.call(0x1800cd030)
    game, mapc, cells, citizen, desc, home, residence, priest, slot = [
        e.malloc(n) for n in (0x160, 0x100, 0x30 * 16, 0x300, 0x100, 0x40, 0x80, 0x40, 8)]
    e.write(0x1401cbdd8, struct.pack('<Q', game))
    e.write(game + 0xf8, struct.pack('<Q', mapc))
    e.write(mapc + 0x80, struct.pack('<Q', cells))
    e.write(citizen + 0x288, struct.pack('<Q', desc))
    e.write(slot, struct.pack('<Q', residence))
    e.hook_function(0x14008d480, 'entrance cell', lambda a: CELL)

    def residence_of(a):
        e.write(a[1], struct.pack('<Q', residence))
        return a[1]
    e.hook_function(0x140008090, 'get<ResidenceComponent>', residence_of)
    r = random.Random(0x140117fb0)
    os.makedirs(OUT, exist_ok=True)
    with open(f'{OUT}/cases.tsv', 'w') as out:
        out.write('flags\thappiness\tfield_84\tfoods\tpriest\thome\tlayers\tfamilies\tmax\ta8\tac\tb0\tafter\n')
        for k in range(cases):
            flags = r.randrange(0, 1 << 19)
            mx = f32(r.choice([10.0, r.uniform(5, 12)]))
            h = f32(r.choice([r.uniform(0, mx), r.uniform(-3, 0), mx + r.uniform(0, 3), 0.0, -0.0,
                              float('nan') if k % 40 == 0 else 5.0]))
            f84, foods = r.choice([0, 5, 12]), r.randrange(0, 8)
            has_priest, has_home = r.random() < 0.4, r.random() < 0.6
            layers = [r.choice([0, 0, 1, 3]) for _ in range(5)] + [r.randrange(0, 4)]
            families = r.choice([1, 2])
            a8, ac, b0 = r.randrange(0, 4), r.randrange(2, 7), r.choice([10, 0, 5])
            e.write(citizen + 0x88, struct.pack('<I', flags))
            e.write(citizen + 0x80, struct.pack('<f', h))
            e.write(citizen + 0x84, struct.pack('<i', f84))
            e.write(citizen + 0xa0, struct.pack('<i', foods))
            e.write(citizen + 0x230, struct.pack('<Q', priest if has_priest else 0))
            e.write(citizen + 0x238, struct.pack('<Q', home if has_home else 0))
            e.write(cells + CELL * 0x30 + 0x1c, bytes(layers))
            e.write(residence + 0x58, struct.pack('<i', families))
            e.write(desc + 0xa4, struct.pack('<f', mx))
            e.write(desc + 0xa8, struct.pack('<iii', a8, ac, b0))
            e.call(0x140117fb0, citizen)
            after = struct.unpack('<I', e.read(citizen + 0x80, 4))[0]
            row = [flags, f'{bits(h):x}', f84, foods, int(has_priest), int(has_home),
                   ','.join(map(str, layers)), families, f'{bits(mx):x}', a8, ac, b0, f'{after:x}']
            out.write('\t'.join(map(str, row)) + '\n')
    print(f'{cases} cases written to {OUT}/cases.tsv')


if __name__ == '__main__':
    main()
