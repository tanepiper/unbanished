"""GrowthComponent and entity-scheduler ground truth from the original x64 binaries.

Usage: .venv/bin/python tools/emu/growth.py OUT_DIR

Writes growth.tsv (initialization, scheduled growth, spawn interval counts, map transition and
visual smoothing) and scheduler.tsv (five-bucket visits). Dumps stay under game-data/derived/emu.
NaN temperatures, values, maxima and fractions are among the cases. Rendering, entity
spawning and owner-mask callbacks are stubbed; arithmetic and interval branches
execute in the original functions. System::Timer::Startup establishes its original microsecond ticks.
"""
import itertools
import os
import struct
import sys

from pe_emu import Emu, F32


def main(out_dir):
    os.makedirs(out_dir, exist_ok=True)
    emu = Emu('game-data/original/Application-steam-x64.exe',
              'game-data/original/Runtime-steam-x64.dll')
    emu.hook_function(0x1800cd0d0, 'platform performance-counter initialization', lambda a: 1)
    emu.call(0x1800cd030)
    seconds_per_tick = struct.unpack('<d', emu.read(0x180202728, 8))[0]
    assert seconds_per_tick == 1e-6
    emu.hook_function(emu.exports['?CheckPtr@InternalBasePtr@Object@@IEBAXXZ'],
                      'CheckPtr (valid synthetic objects)', lambda a: 0)
    emu.hook_function(0x1400c8940, 'owner capability notification', lambda a: 0)
    game, environment, map_obj = [emu.malloc(n) for n in (0x160, 0x78, 0x100)]
    map_cells = emu.malloc(0x30)
    owner, map_component, natural, model, component, description = [
        emu.malloc(n) for n in (0x48, 0x40, 0x40, 0x180, 0x50, 0x68)]
    random = emu.malloc(2504)
    time = emu.malloc(8)
    emu.write(0x1401cbdd8, struct.pack('<Q', game))
    emu.write(game + 0xd8, struct.pack('<Q', random))
    emu.write(game + 0xf8, struct.pack('<Q', map_obj))
    emu.write(game + 0x130, struct.pack('<Q', environment))
    emu.write(map_obj, struct.pack('<i', 1))
    emu.write(map_obj + 0x80, struct.pack('<Q', map_cells))
    emu.write(owner + 0x28, struct.pack('<Q', description))

    def find_map(args):
        emu.write(args[1], struct.pack('<Q', map_component))
        return args[1]

    emu.hook_function(0x140004680, 'map-component lookup', find_map)
    emu.hook_function(0x1401258e0, 'single-cell map index', lambda a: 0)
    attempts = [0]
    emu.hook_function(0x140125070, 'spawn attempt counter',
                      lambda a: attempts.__setitem__(0, attempts[0] + 1) or 0)
    emu.hook_function(0x14012bc50, 'transition dispatch (outside arithmetic)', lambda a: 0)

    def u32(addr, value):
        emu.write(addr, struct.pack('<I', value))

    def f32(addr, value):
        emu.write(addr, struct.pack('<f', value))

    def bits(addr):
        return str(emu.u32(addr))

    rows = []
    for rate, seasonal, temperature, value, maximum, dt, timer, tall, allow, disabled in itertools.product(
        (2.0, -2.0), (0, 1), (20.0, 32.0, 50.0, 60.0, 100.0, float('nan')),
        (0.0, 0.75, 1.0, 1.5, float('nan')), (1.0, 4.0, -1.0, float('nan')),
        (0.0, 0.25, 300.0, 901.0), (0, 300_000_000), (0, 1), (0, 1), (0, 1)):
        emu.write(description, bytes(0x68))
        f32(description + 0x28, rate)
        emu.write(description + 0x2c, bytes([seasonal]))
        f32(description + 0x30, 60.0)
        f32(description + 0x34, 20.0)
        f32(description + 0x40, 0.5)
        emu.write(description + 0x44, b'\x01')
        u32(description + 0x50, 1)
        f32(description + 0x54, 0.75)
        f32(description + 0x60, 0.1)
        emu.write(description + 0x64, bytes([tall]))
        emu.write(component, bytes(0x50))
        emu.write(component + 0x10, struct.pack('<Q', owner))
        u32(component + 0x20, 0xcb)
        emu.write(component + 0x28, struct.pack('<QQQqff',
                  natural, model, description, timer, min(value, 1.0), maximum))
        u32(natural + 0x20, allow << 20)
        f32(natural + 0x28, value)
        f32(environment + 0x18, temperature)
        u32(game + 0xd0, disabled << 10)
        emu.write(map_cells + 0x2c, b'\x80')
        emu.call(emu.exports['??0Time@System@@QEAA@M@Z'], time, F32(dt))
        attempts[0] = 0
        emu.call(0x140125520, component, time)
        # Only invoke the follow-up while capability bit 0x10 is set, as the manager does.
        if emu.u32(component + 0x20) & 0x10:
            emu.call(0x140125370, component)
        output = [bits(natural + 0x28), str(emu.u64(component + 0x40)),
                  bits(component + 0x48), str(emu.u32(component + 0x20) & 0x18),
                  str(attempts[0]), str(emu.read(map_cells + 0x2c, 1)[0])]
        # Terminal transitions don't publish a new scale; compare scales only for live growth.
        output += [bits(model + 0x134 + 4 * i) for i in range(4)] if (
            emu.u32(component + 0x20) & 8) else ['0'] * 4
        rows.append('\t'.join(map(str, [rate, seasonal, temperature, value, maximum, dt,
                      timer, tall, allow, disabled])) + '\t' + '\t'.join(output))
    # Initial fraction/maximum and randomized timer, including both scale shapes.
    init = []
    for seed, maximum, fraction, tall in itertools.product(range(1, 21), (1.0, 4.0, -1.0, float('nan')),
                                                          (0.0, 0.15, 0.5, 1.0, float('nan')), (0, 1)):
        emu.call(emu.exports['??0Random@System@@QEAA@I@Z'], random, seed)
        emu.write(description + 0x64, bytes([tall]))
        f32(component + 0x4c, maximum)
        emu.call(0x140126280, component, F32(fraction))
        init.append('\t'.join(map(str, [seed, maximum, fraction, tall,
            emu.u32(natural + 0x28), emu.u64(component + 0x40), emu.u32(component + 0x48)] +
            [emu.u32(model + 0x134 + 4 * i) for i in range(4)])))
    with open(os.path.join(out_dir, 'growth.tsv'), 'w') as f:
        f.write('\n'.join(rows) + '\n')
    with open(os.path.join(out_dir, 'initialise.tsv'), 'w') as f:
        f.write('\n'.join(init) + '\n')
    print(f'wrote {len(rows)} growth cases and {len(init)} initialization cases')

    manager = emu.malloc(0xe8)
    emu.call(0x1400c9460, manager)
    quotas = list(struct.unpack('<5I', emu.read(manager + 0xa8, 20)))
    assert quotas == [50, 15, 8, 5, 2], quotas
    visits = []
    identifiers = {}

    def visit(args):
        visits.append((identifiers[args[0]], emu.u64(args[1])))
        return 0

    callback = emu._new_stub('scheduled-component visit', visit)
    vtable = emu.malloc(0x50)
    emu.write(vtable + 0x48, struct.pack('<Q', callback))
    next_id = 0
    for bucket, count in enumerate([53, 18, 11, 8, 5]):
        array = emu.malloc(count * 8)
        for slot in range(count):
            entity, part, parts = [emu.malloc(n) for n in (0x48, 0x28, 8)]
            emu.write(entity + 0x30, struct.pack('<Qi', parts, 1))
            u32(entity + 0x40, 8)
            emu.write(parts, struct.pack('<Q', part))
            emu.write(part, struct.pack('<Q', vtable))
            u32(part + 0x20, 8)
            identifiers[part] = next_id
            next_id += 1
            emu.write(array + slot * 8, struct.pack('<Q', entity))
        emu.write(manager + 0x18 + bucket * 16, struct.pack('<Qii', array, count, count))
    scheduled = []
    for step in range(8):
        visits.clear()
        emu.call(emu.exports['??0Time@System@@QEAA@M@Z'], time, F32(0.25))
        emu.call(0x1400c9560, manager, time)
        scheduled.extend(f'{step}\t{i}\t{delta}' for i, delta in visits)
    with open(os.path.join(out_dir, 'scheduler.tsv'), 'w') as f:
        f.write('\n'.join(scheduled) + '\n')
    print(f'wrote {len(scheduled)} scheduler visits')


if __name__ == '__main__':
    main(sys.argv[1])
