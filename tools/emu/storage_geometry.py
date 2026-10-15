"""Emulate entrance, market use-point and nearest storage-slot geometry.

Usage: .venv/bin/python tools/emu/storage_geometry.py OUT_DIR

Runs FUN_14008d480, FUN_14004f710 and FUN_140132aa0 on 8 synthetic 32x32 maps,
using the original region labeller, sector graph, path rank and Random. Component
lookups/point transforms are injected, as is flat ground (z=0); distance-cache writes
are disabled. Also runs FUN_1400c7e70 facing. Outputs geometry.tsv and facing.tsv.
"""
import os
import random
import struct
import sys
sys.path.insert(0, 'tools/emu')
from pe_emu import Emu


def main(out):
    os.makedirs(out, exist_ok=True)
    e = Emu('game-data/original/Application-steam-x64.exe', 'game-data/original/Runtime-steam-x64.dll')
    def alloc(n):
        p = e.malloc(n); e.write(p, bytes(n)); return p
    q = lambda p, v: e.write(p, struct.pack('<Q', v))
    i = lambda p, v: e.write(p, struct.pack('<i', v))
    def result(a, p):
        q(a[1], p); return a[1]
    e.hook_function(e.exports['?CheckPtr@InternalBasePtr@Object@@IEBAXXZ'], 'synthetic identity', lambda a: 0)
    e.hook_function(0x1400ab000, "disable distance cache writes", lambda a: 0)
    points, interact = [], alloc(0x30)
    model = alloc(0x180)
    has_model = False
    facing_lines = []
    e.hook_function(0x140003f60, "get model", lambda a: result(a, model if has_model else 0))
    rect, entity, location, ref, pos = alloc(0x40), alloc(0x40), alloc(0x30), alloc(8), alloc(12)
    has_interact = False
    e.hook_function(0x140002ed0, 'get interact', lambda a: result(a, interact if has_interact else 0))
    e.hook_function(0x140004680, 'get map component', lambda a: result(a, rect))
    e.hook_function(0x140128540, 'use-point count', lambda a: len(points))
    e.hook_function(0x1401282f0, 'transformed use point', lambda a: e.write(a[1], struct.pack('<fff', *points[a[3]])) or a[1])
    e.hook_function(0x1400a48b0, 'flat ground', lambda a: e.write(a[1], e.read(a[2], 8) + bytes(4)) or a[1])
    q(location + 0x10, entity); q(ref, entity)
    game, rnd = alloc(0x200), alloc(2500)
    q(0x1401cbdd8, game); q(game + 0xd8, rnd)
    rng, lines, n = random.Random(839), [], 32
    signed = lambda v: struct.unpack('<i', struct.pack('<I', v & 0xffffffff))[0]
    for map_index in range(8):
        types = [8 if rng.random() < 0.22 else rng.choice([0, 1, 2, 10, 11]) for _ in range(n*n)]
        # Include disconnected islands to exercise path-rank fallback.
        if map_index % 2:
            for y in range(n): types[y*n+16] = 8
        cells, m, graph, cache = alloc(0x30*n*n), alloc(0x200), alloc(0x108), alloc(0x38)
        e.write(cells, b''.join(bytes(0x29)+bytes([t])+bytes(6) for t in types))
        i(m, n); e.write(m+0x80, struct.pack('<Qi', cells, n*n)); e.write(m+0xa2, b'\1')
        e.write(m+0xd0, struct.pack('<ii', 8, 0x18))
        e.call(0x1400adb40, graph, m, 0, 0, n//8, n//8, timeout_insns=500_000_000)
        e.call(0x1400adf60, graph, timeout_insns=500_000_000)
        q(m+0x70, graph); q(m+0x78, cache); q(game+0xf8, m)
        lines.append('map\t'+bytes(types).hex())
        for case in range(30):
            seed = map_index*30+case+1
            e.call(e.exports['??0Random@System@@QEAA@I@Z'], rnd, seed)
            x,y,w,h = rng.randrange(-2,30),rng.randrange(-2,30),rng.randrange(1,5),rng.randrange(1,5)
            # Entrance must be a valid cell for the coarse query's caller contract.
            x,y = max(0,min(x, n-w)),max(0,min(y,n-h))
            e.write(rect+0x28, struct.pack('<hhhh',x,y,w,h))
            has_interact = case % 3 != 0
            points = [(rng.randrange(n)-15.5, rng.randrange(n)-15.5, 0.0) for _ in range(rng.randrange(0,5))] if has_interact else []
            from_cell = rng.randrange(n*n)
            entrance = signed(e.call(0x14008d480, ref, 0))
            e.write(pos, bytes(12))
            rank = signed(e.call(0x140132aa0, location, from_cell, pos, timeout_insns=50_000_000))
            target = struct.unpack('<fff', e.read(pos,12))
            market_rank, market_target = -1, (0.,0.,0.)
            if points:
                e.write(pos, bytes(12))
                market_rank = signed(e.call(0x14004f710, location, from_cell, pos, timeout_insns=50_000_000))
                market_target = struct.unpack('<fff', e.read(pos,12))
            has_model = case % 2 == 0
            source_position = (float(x)-15.75, float(y)-15.25, 1.75)
            e.write(model+0x158, struct.pack('<fff', *source_position))
            if not has_model: source_position = (x+(w//2)-15.5,y+(h//2)-15.5,0.0)
            facing_target = points[0] if points else (0.25, -0.75, 0.5)
            e.write(pos, struct.pack('<fff',*facing_target))
            facing = e.call_f32(0x1400c7e70,pos,ref)
            facing_lines.append(','.join(map(str,source_position))+'\t'+','.join(map(str,facing_target))+'\t'+str(struct.unpack('<I',struct.pack('<f',facing))[0]))
            next_rand = e.call(e.exports['?Rand@Random@System@@QEAAIXZ'], rnd) & 0xffffffff
            fmt = lambda p: ','.join(str(v) for v in p)
            lines.append('\t'.join(map(str,[seed,from_cell,int(has_interact),fmt((x,y,w,h)),';'.join(fmt(p) for p in points),entrance,rank,fmt(target),market_rank,fmt(market_target),next_rand])))
    open(os.path.join(out,'geometry.tsv'),'w').write('\n'.join(lines)+'\n')
    open(os.path.join(out,'facing.tsv'),'w').write('\n'.join(facing_lines)+'\n')
    print('wrote 240 entrance/storage-slot/market/facing queries')


if __name__ == '__main__': main(sys.argv[1])
