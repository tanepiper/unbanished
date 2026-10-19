"""Ground truth for the decals' 2D transforms (apps/game/src/gfx/building_decals.rs `Transform2`):
the Runtime's `Transform2(Scalar, Vector2)` (`0x18007e770`), `Transform2 * Transform2`
(`0x180079120`) and `Transform2 * Vector2` (`0x18006d3b0`).

Usage: .venv/bin/python tools/emu/transform2.py OUT_DIR [COUNT]

Writes `OUT_DIR/transform2.tsv`, hex f32 bits: `yaw1 ox1 oy1 yaw2 ox2 oy2 px py`, then the first
rotation's six floats, the product's six and the transformed point.
"""
import os
import random
import struct
import sys

sys.path.insert(0, 'tools/emu')
from pe_emu import Emu  # noqa: E402


def h(x):
    return '%08x' % struct.unpack('<I', struct.pack('<f', x))[0]


def f32(x):
    return struct.unpack('<f', struct.pack('<f', x))[0]


def main(out_dir, count=5000):
    os.makedirs(out_dir, exist_ok=True)
    emu = Emu('game-data/original/Application-steam-x64.exe',
              'game-data/original/Runtime-steam-x64.dll')
    buf = emu.malloc(0x200)
    rng = random.Random(21)
    rows = []
    for i in range(int(count)):
        yaws = [f32(rng.choice([0.0, 90.0, 180.0, 270.0, -90.0]) if i % 5 == 0
                    else rng.uniform(-360, 360)) for _ in range(2)]
        offs = [f32(rng.uniform(-300, 300)) for _ in range(4)]
        p = [f32(rng.uniform(-20, 20)) for _ in range(2)]
        emu.write(buf + 0x00, struct.pack('<f', yaws[0]))
        emu.write(buf + 0x04, struct.pack('<2f', offs[0], offs[1]))
        emu.write(buf + 0x0c, struct.pack('<f', yaws[1]))
        emu.write(buf + 0x10, struct.pack('<2f', offs[2], offs[3]))
        emu.write(buf + 0x18, struct.pack('<2f', *p))
        emu.call(0x18007e770, buf + 0x40, buf + 0x00, buf + 0x04)   # a at 0x40
        emu.call(0x18007e770, buf + 0x60, buf + 0x0c, buf + 0x10)   # b at 0x60
        emu.call(0x180079120, buf + 0x40, buf + 0x80, buf + 0x60)   # a * b at 0x80
        emu.call(0x18006d3b0, buf + 0x80, buf + 0xa0, buf + 0x18)   # (a * b) * p at 0xa0
        a = struct.unpack('<6f', emu.read(buf + 0x40, 24))
        ab = struct.unpack('<6f', emu.read(buf + 0x80, 24))
        q = struct.unpack('<2f', emu.read(buf + 0xa0, 8))
        ins = [yaws[0], offs[0], offs[1], yaws[1], offs[2], offs[3], *p]
        rows.append('\t'.join(' '.join(map(h, v)) for v in (ins, a, ab, q)))
    with open(os.path.join(out_dir, 'transform2.tsv'), 'w') as f:
        f.write('\n'.join(rows) + '\n')
    print(f'wrote {len(rows)} transforms')


if __name__ == '__main__':
    main(sys.argv[1], *sys.argv[2:])
