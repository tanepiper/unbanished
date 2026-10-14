"""Original animation pose, clock and event ground truth using local shipped resources.

Usage: .venv/bin/python tools/emu/animation.py OUT_DIR

Writes poses.bin (all 64 clips, integer/fractional samples and completion/loop boundaries) and
clocks.tsv (unique AnimationState clocks, IsAnimating and real event interval dispatch). Only
rendering/node publication and OS timer startup are stubbed. Original arithmetic runs in Runtime.
"""
import os
from pathlib import Path
import struct
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
from tools.crs.crs import read_resource
import tools.crs.types.graphics  # registers the complete AnimationGroup decoder
from pe_emu import Emu, F32


def main(out_dir):
    os.makedirs(out_dir, exist_ok=True)
    emu = Emu('game-data/original/Application-steam-x64.exe',
              'game-data/original/Runtime-steam-x64.dll')
    emu.hook_function(0x1800cd0d0, 'platform counter startup', lambda a: 1)
    emu.call(0x1800cd030)
    tick = struct.unpack('<d', emu.read(0x180202728, 8))[0]
    assert tick == 1e-6
    emu.hook_function(0x1800ff4b0, 'pose publication (clock test only)', lambda a: 0)
    emu.hook_function(0x1800ff100, 'root-only pose publication', lambda a: 0)
    time, delta, root = [emu.malloc(n) for n in (8, 8, 0x1c)]
    emu.write(root, struct.pack('<7f', 0, 0, 0, 1, 0, 0, 0))
    node_array = emu.malloc(16)
    state, entry, mesh = [emu.malloc(n) for n in (0x18, 0x38, 0x120)]
    emu.write(state + 8, struct.pack('<Qi', entry, 1))
    emu.write(mesh + 0xe0, struct.pack('<Qi', entry, 1))
    event_ids, emitted = {}, []

    def event(args):
        emitted.append(event_ids[args[0]])
        return 0

    callback = emu._new_stub('animation event observation', event)
    vtable = emu.malloc(0x48)
    emu.write(vtable + 0x40, struct.pack('<Q', callback))
    paths = sorted(Path('game-data/extracted/Models').rglob('*Anims_resource.crs'))
    cases, clock_rows = 0, []
    with open(os.path.join(out_dir, 'poses.bin'), 'wb') as output:
        for path in paths:
            data = read_resource(path.read_bytes())['object']
            key = path.relative_to('game-data/extracted').as_posix()
            group = emu.malloc(0x58)
            clips = emu.malloc(0x70 * len(data['clips']))
            emu.write(group + 0x20, struct.pack('<Qi', clips, len(data['clips'])))
            for index, clip in enumerate(data['clips']):
                address = clips + index * 0x70
                samples = emu.malloc(28 * len(clip['samples']))
                emu.write(samples, b''.join(struct.pack('<7f', *s) for s in clip['samples']))
                emu.write(address + 0x30, struct.pack('<iif', clip['start_frame'], clip['end_frame'], clip['declared_rate']))
                emu.write(address + 0x3c, bytes([clip['looping']]))
                emu.write(address + 0x40, struct.pack('<iffii', clip['loop_start_frame'],
                    clip['sample_rate'], clip['sample_interval'], clip['bone_count'], clip['frame_count']))
                emu.write(address + 0x58, struct.pack('<qQ', clip['duration_ticks'], samples))
                events = emu.malloc(16 * len(clip['events']))
                for i, e in enumerate(clip['events']):
                    obj = emu.malloc(8)
                    emu.write(obj, struct.pack('<Q', vtable))
                    event_ids[obj] = i
                    emu.write(events + i * 16, struct.pack('<i4xQ', e['frame'], obj))
                emu.write(address, struct.pack('<Qi', events, len(clip['events'])))
                pose = emu.malloc(28 * clip['bone_count'])
                interpolated = emu.malloc(56 * clip['bone_count'])
                emu.write(node_array + 8, struct.pack('<i', clip['bone_count']))
                # Packed output: path length + path, clip index, time ticks, delta ticks, bone
                # count, then native-convention Transform bytes. Rust conjugates for comparison.
                duration = clip['duration_ticks']
                times = {0, 1, duration - 1, duration, duration + 1, duration * 3 + 71}
                times.update(int(((frame + fraction) / clip['sample_rate']) / tick)
                    for frame in range(clip['frame_count']) for fraction in (0.0, 0.37))
                for t in sorted(times):
                    for dt in (0, 33_333, 500_000):
                        emu.write(time, struct.pack('<q', t))
                        emu.write(delta, struct.pack('<q', dt))
                        emu.call(0x1800fd530, address, time, delta, pose, clip['bone_count'])
                        name = key.encode()
                        output.write(struct.pack('<I', len(name)) + name +
                            struct.pack('<IqqI', index, t, dt, clip['bone_count']) +
                            emu.read(pose, 28 * clip['bone_count']))
                        cases += 1
                for initial in (0, max(0, duration - 1), duration, duration * 2 + 3):
                    for dt in (0, 1, 33_333, duration + 123):
                        for speed in (1.0, 0.5, 2.0):
                            emu.write(entry, struct.pack('<Qii f4x qqq B7x',
                                group, index, -1, speed, initial, 0, 0, 0))
                            emu.write(delta, struct.pack('<q', dt))
                            emitted.clear()
                            emu.call(0x1801003b0, state, delta, node_array, root,
                                interpolated, pose, root, 0, 1)
                            animating = emu.call(0x180102060, mesh) & 0xff
                            clock_rows.append('\t'.join(map(str, [key, index, initial, dt, speed,
                                emu.u64(entry + 0x18), animating, ','.join(map(str, emitted))])))
    with open(os.path.join(out_dir, 'clocks.tsv'), 'w') as output:
        output.write('\n'.join(clock_rows) + '\n')
    print(f'wrote {cases} pose cases and {len(clock_rows)} clock/event cases from {len(paths)} groups')


if __name__ == '__main__':
    main(sys.argv[1])
