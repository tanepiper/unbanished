"""Ground truth for a particle slot's fade (`FUN_140053490`, the ParticleComponent update): the
alpha (system `+0x7c`) moves by `AsSeconds(dt) × rate` (slot `+0x10`); above 1 it is held at 1
and the rate cleared; at 0 or below the slot goes (`FUN_1400532f0`, hooked).

Usage: .venv/bin/python tools/emu/particle_slot.py OUT_DIR [COUNT]

Writes `OUT_DIR/particle_slot.tsv`: one fade per line, `fade_ticks out_step` then the steps,
each `dt:alpha_bits` (hex) or `dt:gone`.
"""
import os
import random
import struct
import sys

sys.path.insert(0, 'tools/emu')
from pe_emu import Emu  # noqa: E402

UPDATE = 0x140053490
REMOVE = 0x1400532f0


def f32(x):
    return struct.unpack('<f', struct.pack('<f', x))[0]


def main(out_dir, count=400):
    os.makedirs(out_dir, exist_ok=True)
    emu = Emu('game-data/original/Application-steam-x64.exe',
              'game-data/original/Runtime-steam-x64.dll')
    emu.hook_function(0x1800cd0d0, 'platform performance-counter initialization', lambda a: 1)
    emu.call(0x1800cd030)
    comp, slots, system, vtable, time = [emu.malloc(n) for n in (0x60, 0x20, 0x100, 0x20, 8)]
    stub = emu._new_stub('system update', lambda a: 0)
    emu.write(vtable, struct.pack('<4Q', stub, stub, stub, stub))
    emu.write(system, struct.pack('<Q', vtable))
    emu.write(comp + 0x38, struct.pack('<Q', slots))
    emu.write(comp + 0x40, struct.pack('<i', 1))

    def remove(args):
        emu.write(slots + 8, struct.pack('<i', -1))
        return 0
    emu.hook_function(REMOVE, 'slot removal', remove)
    rng = random.Random(5)
    rows = []
    for _ in range(int(count)):
        fade = rng.choice([10_000_000, 3_400_000, 1_000_000, rng.randrange(50_000, 20_000_000)])
        seconds = f32(fade * 1e-6)
        out = rng.randrange(0, 400)
        emu.write(slots, struct.pack('<Qiif', system, 0, -1, f32(1.0 / seconds)))
        emu.write(system + 0x7c, struct.pack('<2f', 0.0, 0.0))
        steps = []
        for k in range(4000):
            if k == out:
                emu.write(slots + 0x10, struct.pack('<f', -f32(1.0 / seconds)))
            dt = rng.choice([16666, 16667, 33333, 66666, rng.randrange(1, 200_000)])
            emu.write(time, struct.pack('<q', dt))
            emu.call(UPDATE, comp, time, timeout_insns=5_000_000)
            if struct.unpack('<i', emu.read(slots + 8, 4))[0] < 0:
                steps.append(f'{dt}:gone')
                break
            alpha = emu.read(system + 0x7c, 4)
            steps.append(f'{dt}:{alpha[::-1].hex()}')
        rows.append(f'{fade} {out}\t' + ' '.join(steps))
    with open(os.path.join(out_dir, 'particle_slot.tsv'), 'w') as f:
        f.write('\n'.join(rows) + '\n')
    print(f'wrote {len(rows)} fades')


if __name__ == '__main__':
    main(sys.argv[1], *sys.argv[2:])
