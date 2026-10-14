"""Observe Game/Game's iteration order and integer calendar in the original executable.

Usage: .venv/bin/python tools/emu/game_loop.py OUT_DIR

Writes loop.tsv from FUN_14009b850. Managers/UI/rendering are stubbed and the call stops at
FUN_1400a95b0, after the simulation/smoothing block. Actual pause guards, iteration counts,
calendar addition/rollover and root-only flags execute in the original function.
"""
import itertools
import os
import struct
import sys
from pe_emu import Emu


def main(out_dir):
    os.makedirs(out_dir, exist_ok=True)
    emu = Emu('game-data/original/Application-steam-x64.exe',
              'game-data/original/Runtime-steam-x64.dll')
    game, control, calendar, map_obj, citizens, camera, root, time = [emu.malloc(n)
        for n in (0x160, 0x40, 0x18, 0x100, 0x138, 0x110, 0x30, 8)]
    emu.write(0x1401cbdd8, struct.pack('<Q', game))
    for offset, pointer in [(0x120, control), (0x128, calendar), (0xf8, map_obj),
                            (0x118, citizens), (0xe0, camera)]:
        emu.write(game + offset, struct.pack('<Q', pointer))
    emu.write(calendar + 0x10, struct.pack('<q', 3_600_000_000))
    phases = []
    modal = [False]

    def named(prefix, label, handler):
        matches = [(k, v) for k, v in emu.exports.items() if k.startswith(prefix)]
        assert len(matches) == 1, (prefix, matches)
        emu.hook_function(matches[0][1], label, handler)

    named('?PausesGame@UserInterface@UI@@', 'modal pause', lambda a: int(modal[0]))
    emu.hook_function(0x1400ab1e0, 'map phase', lambda a: phases.append('map') or 0)
    named('?Update@AnimationManager@Graphics@@', 'animation manager phase',
          lambda a: phases.append(f'animations:{a[2] & 0xff}') or 0)
    emu.hook_function(0x1400c9560, 'entity phase',
        lambda a: phases.append(f'entities:{emu.u64(calendar + 8)}') or 0)
    emu.hook_function(0x1400999b0, 'environment phase',
        lambda a: phases.append(f'environment:{emu.u64(calendar + 8)}') or 0)
    emu.hook_function(0x140091780, 'game-data phase', lambda a: phases.append('data') or 0)
    named('?Update@AchievementManager@Achievement@@', 'achievement update', lambda a: 0)
    emu.hook_function(0x1400c98a0, 'followup phase', lambda a: phases.append('followup') or 0)
    emu.hook_function(0x14009d8c0, 'camera transform query', lambda a: root)
    named('?AsTransform3@Transform@Math@@', 'camera transform conversion', lambda a: root)
    named('?SetCameraToWorld@Camera@Graphics@@', 'camera transform publication', lambda a: 0)

    def stop(args):
        phases.append('ui')
        emu.uc.emu_stop()
        return 0

    emu.hook_function(0x1400a95b0, 'end bounded simulation observation', stop)
    rows = []
    for speed, paused, ui_paused, initial, delta in itertools.product((1, 2, 5, 10), (0, 1),
            (0, 1), (0, 3_599_999_999), (0, 33_333, 3_600_000_000)):
        phases.clear()
        modal[0] = bool(ui_paused)
        emu.write(control + 0x30, struct.pack('<B3xi', paused, speed))
        emu.write(calendar, struct.pack('<I4xq', 1, initial))
        emu.write(time, struct.pack('<q', delta))
        emu.call(0x14009b850, game, time)
        rows.append('\t'.join(map(str, [speed, paused, ui_paused, initial, delta,
            emu.u32(calendar), emu.u64(calendar + 8), ','.join(phases)])))
    with open(os.path.join(out_dir, 'loop.tsv'), 'w') as output:
        output.write('\n'.join(rows) + '\n')
    print(f'wrote {len(rows)} original loop/calendar/phase observations')


if __name__ == '__main__':
    main(sys.argv[1])
