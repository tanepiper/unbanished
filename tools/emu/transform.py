"""Ground truth for the model transforms (crates/game/src/math.rs `Transform`): the Runtime's
`Transform::FromYawPitchOffset` (`0x18007e540`), `Transform * Vector3` (`0x18001eae0`),
`Vector3::FromYawPitch` (`0x180022990`), `Quaternion::Rotate` (`0x18001ea20`),
`Vector3::Heading` (`0x18007d3d0`) and `Transform::Inverse` (`0x18005e620`).

Usage: .venv/bin/python tools/emu/transform.py OUT_DIR [COUNT]

Writes `OUT_DIR/transform.tsv`, hex f32 bits: `yaw pitch ox oy oz vx vy vz lyaw` then the
quaternion, the transformed point, the turned heading and the inverse (quaternion, offset).
"""
import os
import random
import struct
import sys

sys.path.insert(0, 'tools/emu')
from pe_emu import Emu  # noqa: E402


def f32(x):
    return struct.unpack('<f', struct.pack('<f', x))[0]


def h(x):
    return '%08x' % struct.unpack('<I', struct.pack('<f', x))[0]


def main(out_dir, count=50000):
    os.makedirs(out_dir, exist_ok=True)
    emu = Emu('game-data/original/Application-steam-x64.exe',
              'game-data/original/Runtime-steam-x64.dll')
    rng = random.Random(77)
    buf = emu.malloc(0x200)
    rows = []
    for i in range(count):
        yaw = f32(rng.choice([0.0, 90.0, 180.0, -90.0, 270.0]) if i % 7 == 0 else rng.uniform(-360, 360))
        pitch = f32(0.0 if i % 3 else rng.uniform(-90, 90))
        off = [f32(rng.uniform(-200, 200)) for _ in range(3)]
        v = [f32(rng.uniform(-12, 12)) for _ in range(3)]
        lyaw = f32(rng.uniform(-180, 180))
        emu.write(buf + 0x00, struct.pack('<f', yaw))
        emu.write(buf + 0x04, struct.pack('<f', pitch))
        emu.write(buf + 0x10, struct.pack('<3f', *off))
        emu.write(buf + 0x20, struct.pack('<3f', *v))
        emu.write(buf + 0x30, struct.pack('<f', lyaw))
        emu.write(buf + 0x34, struct.pack('<f', 0.0))
        emu.call(0x18007e540, buf + 0x40, buf + 0x00, buf + 0x04, buf + 0x10)  # transform at 0x40
        emu.call(0x18001eae0, buf + 0x40, buf + 0x60, buf + 0x20)              # point at 0x60
        emu.call(0x180022990, buf + 0x70, buf + 0x30, buf + 0x34)              # dir at 0x70
        emu.call(0x18001ea20, buf + 0x40, buf + 0x80, buf + 0x70)              # rotated at 0x80
        hd = emu.call_f32(0x18007d3d0, buf + 0x80)
        emu.call(0x18005e620, buf + 0x40, buf + 0xa0)                          # inverse at 0xa0
        inv = struct.unpack('<7f', emu.read(buf + 0xa0, 28))
        q = struct.unpack('<4f', emu.read(buf + 0x40, 16))
        p = struct.unpack('<3f', emu.read(buf + 0x60, 12))
        ins = [yaw, pitch, *off, *v, lyaw]
        rows.append(' '.join(map(h, ins)) + '\t' + ' '.join(map(h, q)) + '\t'
                    + ' '.join(map(h, p)) + '\t' + h(hd) + '\t' + ' '.join(map(h, inv)))
    with open(os.path.join(out_dir, 'transform.tsv'), 'w') as f:
        f.write('\n'.join(rows) + '\n')
    print(f'wrote {len(rows)} transforms')


if __name__ == '__main__':
    main(sys.argv[1], *(int(a) for a in sys.argv[2:]))
