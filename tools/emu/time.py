"""Ground truth for `System::Time` (crates/core/src/scheduler.rs `ticks`, `seconds`): the
constructor from seconds (`0x1800078c0`) and `AsSeconds` (`0x180007900`).

Usage: .venv/bin/python tools/emu/time.py OUT_DIR [COUNT]

Writes `OUT_DIR/time.tsv`: `seconds_bits ticks` (Time(float)) and `ticks seconds_bits`
(AsSeconds), one pair per line, tab-separated.
"""
import os
import random
import struct
import sys

sys.path.insert(0, 'tools/emu')
from pe_emu import Emu, F32  # noqa: E402


def bits(x):
    return '%08x' % struct.unpack('<I', struct.pack('<f', x))[0]


def main(out_dir, count=100000):
    os.makedirs(out_dir, exist_ok=True)
    emu = Emu('game-data/original/Application-steam-x64.exe',
              'game-data/original/Runtime-steam-x64.dll')
    emu.hook_function(0x1800cd0d0, 'platform performance-counter initialization', lambda a: 1)
    emu.call(0x1800cd030)
    t = emu.malloc(8)
    rng = random.Random(3)
    rows = []
    for i in range(int(count)):
        kind = i % 4
        if kind == 0:
            s = rng.uniform(-1e4, 1e4)
        elif kind == 1:
            s = rng.uniform(0, 1)
        elif kind == 2:
            s = rng.choice([1 / 60, 1 / 30, 0.1, 0.25, 0.5, 3.4, 10.0, 3600.0, 36000.0, 1e-6])
        else:
            s = struct.unpack('<f', struct.pack('<I', rng.getrandbits(32) & 0x4fffffff))[0]
        sb = struct.unpack('<f', struct.pack('<f', s))[0]
        emu.call(0x1800078c0, t, F32(sb))
        ticks = struct.unpack('<q', emu.read(t, 8))[0]
        k = rng.choice([rng.randrange(-10**12, 10**12), rng.randrange(0, 10**8),
                        rng.randrange(0, 1 << 62)])
        emu.write(t, struct.pack('<q', k))
        back = emu.call_f32(0x180007900, t)
        rows.append(f'{bits(sb)} {ticks}\t{k} {bits(back)}')
    with open(os.path.join(out_dir, 'time.tsv'), 'w') as f:
        f.write('\n'.join(rows) + '\n')
    print(f'wrote {len(rows)} pairs')


if __name__ == '__main__':
    main(sys.argv[1], *sys.argv[2:])
