"""Ground truth for the name generator (FUN_1400aa000 → FUN_1400a9d50) from the original exe.

Usage: .venv/bin/python tools/emu/names.py OUT_DIR

For each database (Male, Female, Cities) and seeds 1..40, seeds a System::Random, generates 25
consecutive names (citizens: min 3, max 10; cities: min 5, max 20, the raw FUN_1400aa000 result
before the "vill" fix) and writes `OUT_DIR/names.tsv` (`db seed index name`). Consecutive names
from one stream also check that every random draw is consumed in the original order.
"""
import os
import struct
import sys

sys.path.insert(0, 'tools/emu')
from pe_emu import Emu  # noqa: E402

GEN = 0x1400aa000
BLOCK, END = 0xAAAAAAAA, 0xBBBBBBBB


def parse_ndb(data):
    """Return (transitions[(prefix4, boundary, mask)], starts[u16], pool bytes, offsets[u32])."""
    pos = 0

    def u32():
        nonlocal pos
        v = struct.unpack_from('<I', data, pos)[0]
        pos += 4
        return v

    assert u32() == BLOCK
    u32(); u32()  # size, version
    assert u32() == 0x5412A91F
    u32(); u32()  # type id, file version
    for _ in range(u32()):  # dependencies
        n = u32()
        pos += (n & 0x7fffffff) * (1 if n & 0x80000000 else 2) + 8
    assert u32() == BLOCK
    u32(); u32()
    trans = []
    for _ in range(u32()):
        prefix = data[pos:pos + 4]
        boundary = data[pos + 4]
        mask = struct.unpack_from('<I', data, pos + 5)[0]
        trans.append((prefix, boundary, mask))
        pos += 9
    starts = [struct.unpack_from('<H', data, pos + 2 * i)[0] for i in range(u32())]
    pos += 2 * len(starts)
    n = u32()
    pool = data[pos:pos + n]
    pos += n
    offsets = [u32() for _ in range(u32())]
    return trans, starts, pool, offsets


def build(emu, ndb):
    trans, starts, pool, offsets = ndb
    t = emu.malloc(12 * len(trans))
    emu.write(t, b''.join(p + struct.pack('<II', b, m) for p, b, m in trans))
    s = emu.malloc(2 * len(starts))
    emu.write(s, struct.pack(f'<{len(starts)}H', *starts))
    p = emu.malloc(len(pool))
    emu.write(p, pool)
    o = emu.malloc(4 * len(offsets))
    emu.write(o, struct.pack(f'<{len(offsets)}i', *offsets))
    db = emu.malloc(0x60)
    emu.write(db, bytes(0x60))
    emu.write(db + 0x20, struct.pack('<Qi', t, len(trans)))
    emu.write(db + 0x30, struct.pack('<Qi', s, len(starts)))
    emu.write(db + 0x40, struct.pack('<Q', p))
    emu.write(db + 0x50, struct.pack('<Qi', o, len(offsets)))
    return db


def c_string(emu, addr):
    out = b''
    while True:
        b = emu.read(addr + len(out), 1)
        if b == b'\0':
            return out.decode('latin-1')
        out += b


def main(out_dir):
    os.makedirs(out_dir, exist_ok=True)
    emu = Emu('game-data/original/Application-steam-x64.exe',
              'game-data/original/Runtime-steam-x64.dll')
    X = emu.exports
    rows = []
    for name, lo, hi in [('Male', 3, 10), ('Female', 3, 10), ('Cities', 5, 20)]:
        data = open(f'game-data/extracted/Game/Names/{name}_resource.crs', 'rb').read()
        db = build(emu, parse_ndb(data))
        for seed in range(1, 41):
            rnd = emu.malloc(2504)
            emu.call(X['??0Random@System@@QEAA@I@Z'], rnd, seed)
            for i in range(25):
                s = emu.malloc(8)
                emu.write(s, bytes(8))
                emu.call(GEN, s, db, rnd, lo, hi, timeout_insns=50_000_000)
                ptr = emu.u64(s)
                rows.append(f'{name}\t{seed}\t{i}\t{c_string(emu, ptr) if ptr else ""}')
    with open(os.path.join(out_dir, 'names.tsv'), 'w') as f:
        f.write('\n'.join(rows) + '\n')
    print(f'wrote {len(rows)} names')


if __name__ == '__main__':
    main(sys.argv[1])
