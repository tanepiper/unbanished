"""Ground truth for storage search ranking, material selection and inventory mutations.

Usage: .venv/bin/python tools/emu/storage.py OUT_DIR

Runs FUN_140093200 and FUN_140094410 on synthetic entity/component graphs. Geometry
queries are injected (coarse rank, exact slot rank and home distance); object identity
checks are stubbed. The search, both list scans, flags, material preference, reservation
filter and original Random execute unchanged. Writes storage.tsv under ignored OUT_DIR.
"""
import os
import random
import struct
import sys

sys.path.insert(0, 'tools/emu')
from pe_emu import Emu


def main(out):
    os.makedirs(out, exist_ok=True)
    e = Emu('game-data/original/Application-steam-x64.exe',
            'game-data/original/Runtime-steam-x64.dll')
    q = lambda p, v: e.write(p, struct.pack('<Q', v))
    i = lambda p, v: e.write(p, struct.pack('<i', v))
    def alloc(n):
        p = e.malloc(n)
        e.write(p, bytes(n))
        return p
    e.hook_function(e.exports['?CheckPtr@InternalBasePtr@Object@@IEBAXXZ'], 'synthetic identity', lambda a: 0)
    e.hook_function(e.exports['?IsDerivedFrom@Base@Object@@QEBA_NI@Z'], 'synthetic type', lambda a: 1)
    types = [(0x1401cfcb8, 1), (0x1401cc870, 2), (0x1401cfee8, 3)]
    vtables = {}
    for addr, value in types:
        i(addr, value)
        vt, fn = alloc(32), alloc(32)
        e.write(fn, b'\xb8' + struct.pack('<I', value) + b'\xc3')
        q(vt + 16, fn)
        vtables[value] = vt
    def desc(ty, n=0x68):
        p = alloc(n)
        q(p, vtables[ty])
        return p
    def array(owner, offset, xs, stride=8):
        p = alloc(max(1, len(xs)) * stride)
        for k, x in enumerate(xs):
            q(p + stride * k, x)
        q(owner + offset, p)
        i(owner + offset + 8, len(xs))
        return p
    def result(a, value):
        q(a[1], value)
        return a[1]
    game, map_, rnd = alloc(0x200), alloc(0x200), alloc(2500)
    q(0x1401cbdd8, game)
    q(game + 0xf8, map_)
    q(game + 0xd8, rnd)
    i(map_, 32)
    inventories, candidates, metrics = {}, {}, {}
    e.hook_function(0x140006270, 'get storage', lambda a: result(a, inventories[a[0]]))
    e.hook_function(0x1400182d0, 'get market', lambda a: result(a, a[0]))
    e.hook_function(0x14008d480, 'entrance cell', lambda a: candidates[e.u64(a[0])])
    e.hook_function(0x1400a4ce0, 'injected path rank', lambda a: metrics[a[1]][1 if a[2] == 1000 else 0])
    def position(a):
        entity = a[0]
        cell = candidates[entity]
        e.write(a[2], struct.pack('<fff', float(cell), 2.0, 3.0))
        return metrics[cell][2]
    e.hook_function(0x14004f710, 'injected market slot', position)
    e.hook_function(0x140132aa0, 'injected storage slot', lambda a: position([e.u64(a[0] + 0x10), a[1], a[2]]))
    rng = random.Random(20261015)
    lines = []
    for case in range(600):
        seed, category = case + 1, rng.randrange(5)
        e.call(e.exports['??0Random@System@@QEAA@I@Z'], rnd, seed)
        materials = []
        for _ in range(4):
            flags = rng.choice([0x20, 0x40, 0x200, 0x2000, 0x4000])
            preferred = rng.randrange(5) == 0
            d, template = desc(1), alloc(0x40)
            i(d + 0x5c, flags)
            e.write(d + 0x58, bytes([preferred]))
            array(template, 0x28, [d])
            materials.append((template, flags, preferred))
        manager, radius, storage, specs, owners = alloc(0x30), [], [], [], []
        for k in range(rng.randrange(1, 10)):
            is_market = rng.randrange(3) == 0
            coarse, home, exact = rng.randrange(1, 80), rng.randrange(1, 80), rng.randrange(-1, 80)
            cover = rng.randrange(1, 80)
            accepts = rng.choice([0, 0x20, 0x200, 0x4000, 0xffffffff])
            override = rng.choice([0, 0x20, 0x200, 0x4000, 0xffffffff])
            entity, template, inv = alloc(0x50), alloc(0x40), alloc(0x50)
            inventories[entity] = inv
            i(inv + 0x3c, accepts if accepts < 2**31 else accepts - 2**32)
            entries, fields = [], []
            for m, (mt, _, _) in enumerate(materials):
                count, reserved = rng.randrange(0, 5), rng.randrange(0, 5)
                entries.append((mt, count, reserved))
                fields.append(f'{count}:{reserved}')
            buf = alloc(16 * len(entries))
            e.write(buf, b''.join(struct.pack('<Qii', *x) for x in entries))
            q(inv + 0x28, buf)
            i(inv + 0x30, len(entries))
            q(entity + 0x28, template)
            d = desc(2)
            i(d + 0x28, cover)
            array(template, 0x28, [d])
            location, ld = desc(3), alloc(0x28)
            q(location + 0x10, entity)
            q(location + 0x18, ld)
            i(ld + 0x20, override if override < 2**31 else override - 2**32)
            array(entity, 0x30, [location])
            # Storage helper takes the component; market helper takes the owner.
            candidates[entity] = k + 1
            metrics[k + 1] = coarse, home, exact
            (radius if is_market else storage).append(entity)
            owners.append(entity)
            specs.append(':'.join(map(str, [int(is_market), coarse, home, exact, cover, accepts, override])) + ',' + ','.join(fields))
        array(manager, 0, storage)
        array(manager, 0x10, radius)
        owner_out, material_out, pos = alloc(8), alloc(8), alloc(12)
        e.call(0x140093200, manager, owner_out, 999, 1000 if case % 3 else -1, material_out, pos, category)
        owner, mat = e.u64(owner_out), e.u64(material_out)
        selected = owners.index(owner) if owner else -1
        material = next((j for j, x in enumerate(materials) if x[0] == mat), -1) if owner else -1
        next_rand = e.call(e.exports['?Rand@Random@System@@QEAAIXZ'], rnd) & 0xffffffff
        material_spec = ','.join(f'{f}:{int(p)}' for _, f, p in materials)
        lines.append('\t'.join(map(str, [seed, category, int(case % 3 != 0), material_spec, ';'.join(specs), selected, material, next_rand])))
    with open(os.path.join(out, 'storage.tsv'), 'w') as f:
        f.write('\n'.join(lines) + '\n')
    print(f'wrote {len(lines)} original storage searches')


if __name__ == '__main__':
    main(sys.argv[1])
