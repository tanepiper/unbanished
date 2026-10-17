"""Ground truth for the characters' spatial tree (`Game +0x88`): build, insert, move, remove
and both searches, run on the original functions.

Usage: .venv/bin/python tools/emu/character_tree.py OUT_DIR

Builds the tree (`FUN_140018cc0`, 2048 units), then plays a random script of operations on
fake CharacterComponents (position at the owner transform's `+0x158`, radius at the
description's `+0x20`): insert (`FUN_140018e00`), move (`FUN_140019070`), remove
(`FUN_140018f70`), `within` (`FUN_140097e60`, type 0) and `nearest` (`FUN_140097730`, type 0,
nothing excluded). Writes `OUT_DIR/character_tree.tsv`, one operation per line:

    insert ID X Y Z R SLOT | move ID X Y Z | remove ID | within X Y Z R IDS | nearest X Y Z R ID

with IDS a comma list (`-` for none) and ID `-1` for none.
"""
import os
import random
import struct
import sys

sys.path.insert(0, 'tools/emu')
from pe_emu import Emu, F32  # noqa: E402

BUILD = 0x140018cc0
INSERT = 0x140018e00
REMOVE = 0x140018f70
PLACE = 0x140019070
WITHIN = 0x140097e60
NEAREST = 0x140097730


def main(out_dir):
    os.makedirs(out_dir, exist_ok=True)
    emu = Emu('game-data/original/Application-steam-x64.exe',
              'game-data/original/Runtime-steam-x64.dll')
    tree = emu.malloc(0x100)
    emu.call(BUILD, tree, F32(2048.0), 0, 0)
    rng = random.Random(4321)
    comps = {}   # id -> (component address, transform address)
    slots = {}   # id -> slot
    lines = []

    def make(i, pos, r):
        comp = emu.malloc(0x100)
        transform = emu.malloc(0x200)
        desc = emu.malloc(0x40)
        entity = emu.malloc(0x40)
        emu.write(comp + 0x10, struct.pack('<Q', entity))
        emu.write(comp + 0x68, struct.pack('<Q', transform))
        emu.write(comp + 0x78, struct.pack('<Q', desc))
        emu.write(desc + 0x20, struct.pack('<f', r))
        emu.write(transform + 0x158, struct.pack('<3f', *pos))
        comps[i] = (comp, transform)

    def point():
        return (rng.uniform(-120, 120), rng.uniform(-120, 120), rng.uniform(-14, 16))

    next_id = 0
    for _ in range(3000):
        op = rng.random()
        live = sorted(slots)
        if op < 0.3 or not live:
            pos = point()
            r = rng.choice([0.25, 0.25, 0.25, 0.5, 1.0, 3.0])
            i = next_id
            next_id += 1
            make(i, pos, r)
            holder = emu.malloc(8)
            emu.write(holder, struct.pack('<Q', comps[i][0]))
            slot = emu.call(INSERT, tree, holder) & 0xFFFFFFFF
            slots[i] = slot
            lines.append(f'insert {i} {pos[0]!r} {pos[1]!r} {pos[2]!r} {r!r} {slot}')
        elif op < 0.65:
            i = rng.choice(live)
            comp, transform = comps[i]
            old = struct.unpack('<3f', emu.read(transform + 0x158, 12))
            if rng.random() < 0.7:
                pos = (old[0] + rng.uniform(-6, 6), old[1] + rng.uniform(-6, 6), old[2])
            else:
                pos = point()
            emu.write(transform + 0x158, struct.pack('<3f', *pos))
            pos = struct.unpack('<3f', emu.read(transform + 0x158, 12))
            emu.call(PLACE, tree, slots[i])
            lines.append(f'move {i} {pos[0]!r} {pos[1]!r} {pos[2]!r}')
        elif op < 0.75:
            i = rng.choice(live)
            emu.call(REMOVE, tree, slots.pop(i))
            lines.append(f'remove {i}')
        else:
            p = point()
            r = rng.choice([8.0, 8.0, 20.0, 40.0])
            pbuf = emu.malloc(16)
            emu.write(pbuf, struct.pack('<3f', *p))
            rbuf = emu.malloc(16)
            emu.write(rbuf, struct.pack('<f', r))
            p = struct.unpack('<3f', emu.read(pbuf, 12))
            by_comp = {c: i for i, (c, _) in comps.items()}
            if op < 0.88:
                ret = emu.malloc(16)
                lst = emu.malloc(16)
                emu.call(WITHIN, tree, ret, pbuf, rbuf, 0, lst, timeout_insns=50_000_000)
                data, count = struct.unpack('<Qi', emu.read(lst, 12))
                ids = [by_comp[emu.u64(data + 8 * k)] for k in range(count)]
                lines.append(f'within {p[0]!r} {p[1]!r} {p[2]!r} {r!r} '
                             + (','.join(map(str, ids)) or '-'))
            else:
                ret = emu.malloc(16)
                excl = emu.malloc(16)
                emu.call(NEAREST, tree, ret, pbuf, rbuf, 0, excl, timeout_insns=50_000_000)
                found = emu.u64(ret)
                lines.append(f'nearest {p[0]!r} {p[1]!r} {p[2]!r} {r!r} '
                             f'{by_comp.get(found, -1)}')
    with open(os.path.join(out_dir, 'character_tree.tsv'), 'w') as f:
        f.write('\n'.join(lines) + '\n')
    print(f'wrote {len(lines)} operations')


if __name__ == '__main__':
    main(sys.argv[1])
