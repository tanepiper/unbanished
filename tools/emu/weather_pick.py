"""The environment's weather pick and yearly reseed, from the original executable.

Usage: .venv/bin/python tools/emu/weather_pick.py [CASES]

Runs FUN_140098ac0 for CASES chosen inputs (default 2000) and writes
game-data/derived/emu/weather_pick/cases.tsv: one line per case with the inputs (seed, the
calendar's time into the year in microseconds at Game +0x128 -> +8, the level +0x14 and the bias
+0x44 before, and the climate at environment +8: baseline +0x34, baseline range +0x40, bias range
+0x104 and the 48 monthly cutoffs from +0x44) and the outputs (state +0x10, level +0x14, the
baseline target +0x30, the bias target +0x48, and the noise's four values +0x34 to +0x40, as
hex), then the next Rand of the game's Random, which shows how many numbers were drawn. About a
third of the cases fall on slice 0 or 24, where the noise and the targets are drawn again.
"""
import os
import random
import struct
import sys

sys.path.insert(0, os.path.dirname(__file__))
from pe_emu import Emu  # noqa: E402

OUT = 'game-data/derived/emu/weather_pick'
SLICE = 3600 * 1_000_000 // 48


def f32(v):
    return struct.unpack('<f', struct.pack('<f', v))[0]


def main():
    cases = int(sys.argv[1]) if len(sys.argv) > 1 else 2000
    e = Emu('game-data/original/Application-steam-x64.exe', 'game-data/original/Runtime-steam-x64.dll')
    e.hook_function(0x1800cd0d0, 'platform counter setup', lambda a: 1)
    e.call(0x1800cd030)
    game, rng, calendar, env, climate = [e.malloc(n) for n in (0x160, 2504, 0x40, 0x100, 0x200)]
    e.write(0x1401cbdd8, struct.pack('<Q', game))
    e.write(game + 0xd8, struct.pack('<Q', rng))
    e.write(game + 0x128, struct.pack('<Q', calendar))
    e.write(env + 8, struct.pack('<Q', climate))
    r = random.Random(0x140098ac0)
    os.makedirs(OUT, exist_ok=True)
    with open(f'{OUT}/cases.tsv', 'w') as out:
        out.write('seed\ttime\tlevel\tbias\tbaseline\tbaseline_range\tbias_range\tmonths\t'
                  'state_after\tlevel_after\tbaseline_target\tbias_target\tnoise\tnext\n')
        for k in range(cases):
            seed = r.randrange(1, 1 << 31)
            if k % 3 == 0:
                time = r.choice([0, 24]) * SLICE + r.randrange(0, SLICE)
            else:
                time = r.randrange(0, 3600 * 1_000_000)
            level = r.randrange(-1, 7)
            bias = f32(r.uniform(-0.3, 0.3))
            if k % 40 == 0:
                bias = float('nan')
            baseline, baseline_range, bias_range = f32(r.uniform(20, 70)), f32(r.uniform(0, 30)), f32(r.uniform(0, 0.5))
            months = []
            for _ in range(12):
                cut = sorted(f32(r.uniform(0.0, 1.0)) for _ in range(3))
                months += cut + [f32(r.uniform(0.0, 1.0))]
            e.write(calendar + 8, struct.pack('<q', time))
            e.write(env + 0x10, struct.pack('<ii', 7, level))
            e.write(env + 0x30, struct.pack('<f', -1.0))
            e.write(env + 0x34, b'\xee' * 16)
            e.write(env + 0x44, struct.pack('<ff', bias, -1.0))
            e.write(climate + 0x34, struct.pack('<f', baseline))
            e.write(climate + 0x40, struct.pack('<f', baseline_range))
            e.write(climate + 0x44, struct.pack('<48f', *months))
            e.write(climate + 0x104, struct.pack('<f', bias_range))
            e.call(e.exports['??0Random@System@@QEAA@I@Z'], rng, seed)
            e.call(0x140098ac0, env)
            state_after, level_after = struct.unpack('<ii', e.read(env + 0x10, 8))
            baseline_target = struct.unpack('<I', e.read(env + 0x30, 4))[0]
            bias_target = struct.unpack('<I', e.read(env + 0x48, 4))[0]
            noise = e.read(env + 0x34, 16).hex()
            after = e.call(e.exports['?Rand@Random@System@@QEAAIXZ'], rng) & 0xffffffff
            out.write('\t'.join(str(x) for x in [
                seed, time, level, repr(bias), repr(baseline), repr(baseline_range), repr(bias_range),
                ','.join(repr(m) for m in months), state_after, level_after, baseline_target, bias_target,
                noise, after]) + '\n')
    print(f'{cases} cases written to {OUT}/cases.tsv')


if __name__ == '__main__':
    main()
