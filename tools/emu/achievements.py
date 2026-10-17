"""Ground truth for the achievement checks (apps/game/src/achievements.rs
`AchievementManager::update_with`, `AchievementInstance::update_with`): the Runtime's
`AchievementManager::Update` (`0x18009f1e0`), `GoalInstance::Update` (`0x18009ff20`) and the
TimeGoalInstance's update and state (`FUN_1400e0100`, `FUN_1400e0180`), over scripted goal states.

Usage: .venv/bin/python tools/emu/achievements.py OUT_DIR [STEPS]

Each plain goal's `CheckGoalState` is a stub that returns the next scripted state. Writes
`OUT_DIR/achievements.tsv`: the setup line (each achievement's goals, `T<years>` for a time
goal and `P` otherwise, `/` between achievements; then the awarded flags as `0`/`1`), then per
step, tab-separated: `dt`, the states handed out in call order (`a,g,state` joined by `;`), and
afterwards the awarded flags, the elapsed times, the cursor and every goal's `flags:time`
(`,` between goals, `/` between achievements).
"""
import os
import random
import struct
import sys

sys.path.insert(0, 'tools/emu')
from pe_emu import Emu  # noqa: E402

UPDATE = 0x18009f1e0
GOAL_UPDATE = 0x18009ff20
TIME_UPDATE = 0x1400e0100
TIME_STATE = 0x1400e0180
SET_MET = 0x18009fb50
IS_DERIVED = 0x1800c2b10
FS_SINGLETON = 0x1802006a8

SETUP = [['P', 'P'], ['P', 'T1'], ['T0'], ['P', 'P', 'P'], ['P', 'T2', 'P']]
AWARDED = [False, False, False, True, False]


def main(out_dir, steps=3000):
    os.makedirs(out_dir, exist_ok=True)
    emu = Emu('game-data/original/Application-steam-x64.exe',
              'game-data/original/Runtime-steam-x64.dll')
    emu.hook_function(0x1800cd0d0, 'platform performance-counter initialization', lambda a: 1)
    emu.call(0x1800cd030)
    emu.hook_function(IS_DERIVED, 'IsDerivedFrom (synthetic objects)', lambda a: 1)
    emu.hook_function(0x1800c2c00, 'IsDerivedFrom by id (synthetic objects)', lambda a: 1)
    fs = emu.malloc(0x100)
    emu.write(FS_SINGLETON, struct.pack('<Q', fs))
    emu.write(fs + 0x38, struct.pack('<i', 0))
    rng = random.Random(17)
    calls = []
    states = {}

    def script(args):
        who = states[args[0]]
        v = rng.choices([0, 1, 2], [70, 29, 1])[0]
        calls.append([who[0], who[1], v])
        return v
    stub = emu._new_stub('CheckGoalState (scripted)', script)
    n = len(SETUP)
    manager = emu.malloc(0x100)
    awarded = emu.malloc(16)
    instances = emu.malloc(8 * n)
    elapsed = emu.malloc(8 * n)
    emu.write(manager, b'\x01')
    emu.write(manager + 0x78, struct.pack('<Q', awarded))
    emu.write(manager + 0x88, struct.pack('<Q', instances))
    emu.write(manager + 0x90, struct.pack('<i', n))
    emu.write(manager + 0x98, struct.pack('<Q', 0))
    emu.write(manager + 0xa0, struct.pack('<Q', elapsed))
    emu.write(manager + 0xb0, struct.pack('<i', 0))
    emu.write(awarded, bytes(AWARDED))
    emu.write(elapsed, b'\0' * 8 * n)

    def set_met(args):
        emu.write(awarded + args[1], b'\x01')
        return 1
    emu.hook_function(SET_MET, 'SetAchievementMet (no Steam)', set_met)
    plain_vt = emu.malloc(0x40)
    emu.write(plain_vt, struct.pack('<8Q', 0, 0, 0, 0, 0, GOAL_UPDATE, stub, 0))
    time_vt = emu.malloc(0x40)
    emu.write(time_vt, struct.pack('<8Q', 0, 0, 0, 0, 0, TIME_UPDATE, TIME_STATE, 0))
    goals = []
    for a, gs in enumerate(SETUP):
        inst = emu.malloc(0x40)
        arr = emu.malloc(8 * len(gs))
        emu.write(inst + 0x10, struct.pack('<Qi', arr, len(gs)))
        emu.write(instances + 8 * a, struct.pack('<Q', inst))
        row = []
        for g, kind in enumerate(gs):
            gi = emu.malloc(0x40)
            desc = emu.malloc(0x40)
            emu.write(gi + 8, struct.pack('<i', 1000))
            if kind == 'P':
                emu.write(gi, struct.pack('<Q', plain_vt))
            else:
                emu.write(gi, struct.pack('<Q', time_vt))
                emu.write(desc + 0x30, struct.pack('<i', int(kind[1:])))
            emu.write(gi + 0x18, struct.pack('<Q', desc))
            emu.write(arr + 8 * g, struct.pack('<Q', gi))
            states[gi] = (a, g)
            row.append(gi)
        goals.append(row)
    time = emu.malloc(8)
    lines = ['/'.join(','.join(g) for g in SETUP) + '\t' + ''.join('01'[a] for a in AWARDED)]
    for _ in range(int(steps)):
        dt = rng.choice([16667, 1_000_000, rng.randrange(1, 4_000_000_000)])
        emu.write(time, struct.pack('<q', dt))
        calls.clear()
        emu.call(UPDATE, manager, time, timeout_insns=10_000_000)
        flags = [[list(struct.unpack('<I4xq', emu.read(gi + 0x10, 4) + b'\0' * 4 + emu.read(gi + 0x20, 8)))
                  for gi in row] for row in goals]
        lines.append('\t'.join([
            str(dt),
            ';'.join(f'{a},{g},{v}' for a, g, v in calls),
            ''.join('1' if b else '0' for b in emu.read(awarded, n)),
            ' '.join(map(str, struct.unpack('<%dq' % n, emu.read(elapsed, 8 * n)))),
            str(struct.unpack('<i', emu.read(manager + 0xb0, 4))[0]),
            '/'.join(','.join(f'{f}:{t}' for f, t in row) for row in flags),
        ]))
    with open(os.path.join(out_dir, 'achievements.tsv'), 'w') as f:
        f.write('\n'.join(lines) + '\n')
    print(f'wrote {len(lines) - 1} steps')


if __name__ == '__main__':
    main(sys.argv[1], *sys.argv[2:])
