"""A sick citizen passing the disease on, from the original executable.

Usage: .venv/bin/python tools/emu/disease_spread.py [CASES]

Runs FUN_140117940 for CASES chosen cases (default 2000) and writes
game-data/derived/emu/disease_spread/cases.tsv: one line per case with the seed, the disease's
spread chance (+0x34 of the disease at citizen +0x68), the citizens the characters' tree returns
(each `s` for the sick citizen itself, `1` for one who has a disease, `0` for one who has none,
in the tree's order), whether the game's Random moved, and the positions in that list of the
citizens offered the disease (FUN_14011c060), in order.

The tree query (FUN_140097e60) is hooked to return the chosen list, get<CitizenComponent>
(FUN_140009790) to map each entity to its synthetic citizen, and the offer to record its citizen.
"""
import os
import random
import struct
import sys

from unicorn.x86_const import UC_X86_REG_RSP

sys.path.insert(0, os.path.dirname(__file__))
from pe_emu import Emu  # noqa: E402

OUT = 'game-data/derived/emu/disease_spread'
MAX = 8


def main():
    cases = int(sys.argv[1]) if len(sys.argv) > 1 else 2000
    e = Emu('game-data/original/Application-steam-x64.exe', 'game-data/original/Runtime-steam-x64.dll')
    e.hook_function(0x1800cd0d0, 'platform counter setup', lambda a: 1)
    e.call(0x1800cd030)
    game, rng, disease, sick, sick_entity, character, model, items, array = [
        e.malloc(n) for n in (0x160, 2504, 0x100, 0x300, 0x40, 0x100, 0x200, 0x20 * MAX, 8 * MAX)]
    entities = [e.malloc(0x40) for _ in range(MAX)]
    citizens = [e.malloc(0x300) for _ in range(MAX)]
    e.write(0x1401cbdd8, struct.pack('<Q', game))
    e.write(game + 0xd8, struct.pack('<Q', rng))
    e.write(game + 0x88, struct.pack('<Q', e.malloc(0x40)))
    e.write(sick + 0x10, struct.pack('<Q', sick_entity))
    e.write(sick + 0x68, struct.pack('<Q', disease))
    e.write(sick + 0x278, struct.pack('<Q', character))
    e.write(character + 0x68, struct.pack('<Q', model))
    e.write(model + 0x158, struct.pack('<fff', 1.0, 2.0, 0.0))
    e.write(disease + 0x38, struct.pack('<f', 6.0))
    by_entity = {sick_entity: sick}
    by_entity.update(zip(entities, citizens))
    found = []

    def query(a, emu):
        # The sixth argument (the result list) is at rsp + 0x30 inside the call.
        out = struct.unpack('<Q', emu.read(emu.uc.reg_read(UC_X86_REG_RSP) + 0x30, 8))[0]
        for i, entity in enumerate(found):
            emu.write(items + 0x20 * i + 0x10, struct.pack('<Q', entity))
            emu.write(array + 8 * i, struct.pack('<Q', items + 0x20 * i))
        emu.write(out, struct.pack('<Qi', array, len(found)))
        return a[1]

    def citizen_of(a, emu):
        emu.write(a[1], struct.pack('<Q', by_entity.get(a[0], 0)))
        return a[1]

    offered = []
    e.hook_function(0x140097e60, 'characters tree query', query)
    e.hook_function(0x140009790, 'get<CitizenComponent>', citizen_of)
    e.hook_function(0x14011c060, 'offer the disease', lambda a: offered.append(a[0]) or 0)
    e.hook_function(e.exports['?MemFree@MemoryAllocator@System@@SAXPEAX@Z'], 'free', lambda a: 0)
    r = random.Random(0x140117940)
    os.makedirs(OUT, exist_ok=True)
    with open(f'{OUT}/cases.tsv', 'w') as out:
        out.write('seed\tchance\tlist\tdrew\toffered\n')
        for k in range(cases):
            seed = r.randrange(1, 1 << 31)
            chance = struct.unpack('<f', struct.pack('<f', r.choice([0.0, 1.0, r.random(), r.random()])))[0]
            kinds = [r.choice('0011s') for _ in range(r.randrange(0, MAX))]
            # At most one entry is the sick citizen itself.
            if kinds.count('s') > 1:
                first = kinds.index('s')
                kinds = [c if c != 's' or i == first else '1' for i, c in enumerate(kinds)]
            found.clear()
            for i, c in enumerate(kinds):
                found.append(sick_entity if c == 's' else entities[i])
                if c != 's':
                    e.write(citizens[i] + 0x68, struct.pack('<Q', disease if c == '1' else 0))
            e.write(disease + 0x34, struct.pack('<f', chance))
            e.call(e.exports['??0Random@System@@QEAA@I@Z'], rng, seed)
            before = e.read(rng, 2504)
            offered.clear()
            e.call(0x140117940, sick)
            drew = e.read(rng, 2504) != before
            positions = [citizens.index(c) for c in offered]
            out.write(f'{seed}\t{chance!r}\t{"".join(kinds) or "-"}\t{int(drew)}\t'
                      f'{",".join(map(str, positions)) or "-"}\n')
    print(f'{cases} cases written to {OUT}/cases.tsv')


if __name__ == '__main__':
    main()
