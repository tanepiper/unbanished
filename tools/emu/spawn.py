"""SpawnComponent timers and child-creation ground truth from the original executable.

Usage: .venv/bin/python tools/emu/spawn.py OUT_DIR

Writes timers.tsv and children.tsv under ignored game-data/derived/emu. Native scheduled spawn,
chance/coordinate draws, model/growth setup, position jitter and growth initialization execute.
Type/owner lookup, allocation, flat-ground admission, density and graphics/registration are
instrumented. Constructor setup is executed ahead at the exact post-coordinate RNG state, then
replayed at the constructor hook, avoiding nested Unicorn calls while preserving draw order.
"""
import itertools
import os
import struct
import sys
from pe_emu import Emu
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
from tools.crs.crs import type_id


class Harness:
    def __init__(self):
        self.emu = e = Emu('game-data/original/Application-steam-x64.exe',
                          'game-data/original/Runtime-steam-x64.dll')
        e.hook_function(0x1800cd0d0, 'platform counter setup', lambda a: 1)
        e.call(0x1800cd030)
        e.hook_function(e.exports['?CheckPtr@InternalBasePtr@Object@@IEBAXXZ'], 'valid synthetic references', lambda a: 0)
        e.hook_function(e.exports['?IsDerivedFrom@Base@Object@@QEBA_NI@Z'], 'valid description identity', lambda a: 1)
        e.hook_function(e.exports['??0Internal@Object@@QEAA@XZ'], 'internal object identity registration', lambda a: a[0])
        e.hook_function(0x1400c8940, 'owner mask notification', lambda a: 0)
        self.game, self.map, self.random, self.owner, self.natural, self.map_component = [
            e.malloc(n) for n in (0x160, 0x100, 2504, 0x48, 0x30, 0x40)]
        self.component, self.description, self.rows = [e.malloc(n) for n in (0x38, 0x30, 5 * 16)]
        e.write(0x1401cbdd8, struct.pack('<Q', self.game))
        self.u64(self.game + 0xd8, self.random); self.u64(self.game + 0xf8, self.map)
        self.u32(self.map, 16)
        e.call(0x140056990, self.component)
        assert e.u32(self.component + 0x20) & 0xffff == 0x88
        self.u64(self.component + 0x10, self.owner); self.u64(self.component + 0x18, self.description)
        self.u64(self.description + 0x20, self.rows); self.u32(self.description + 0x28, 5)
        self.parents_template = e.malloc(0x40)
        self.u64(self.owner + 0x28, self.parents_template); self.u32(self.parents_template + 0x20, 3)
        self.growths, self.templates = [], []
        callback = e._new_stub('GrowthDescription type', lambda a: type_id('GrowthDescription'))
        vt = e.malloc(0x18); self.u64(vt + 0x10, callback)
        self.u32(0x1401cf7e8, type_id('GrowthDescription'))
        for i in range(5):
            growth, template, descriptions = [e.malloc(n) for n in (0x68, 0x40, 8)]
            self.u64(growth, vt); self.u64(descriptions, growth)
            self.u64(template + 0x28, descriptions); self.u32(template + 0x30, 1)
            self.u32(template + 0x20, 4)
            e.write(self.rows + i * 16, struct.pack('<Qff', template, 0.1 + i, 10.0))
            self.growths.append(growth); self.templates.append(template)
        self.child_owner, self.child_natural, self.child_map, self.child_model, self.child_growth, self.model_description, self.natural_description = [
            e.malloc(n) for n in (0x48, 0x30, 0x40, 0x180, 0x50, 0x40, 0x88)]
        self.u64(self.child_natural + 0x10, self.child_owner)
        self.u64(self.child_natural + 0x18, self.natural_description)
        self.u64(self.child_model + 0x18, self.model_description)
        self.u64(self.child_growth + 0x10, self.child_owner)
        self.u64(self.child_growth + 0x18, self.growths[0])
        variants = e.malloc(2 * 0x18)
        meshes = e.malloc(3 * 8)
        for i in range(2):
            e.write(variants + i * 0x18, struct.pack('<QQi4x', 0, meshes, 3))
        self.u64(self.model_description + 0x20, variants); self.u32(self.model_description + 0x28, 2)
        self.u32(self.model_description + 0x34, 0)
        e.write(self.model_description + 0x38, b'\x01')
        self.f32(self.natural_description + 0x60, 0.25)
        for address, parent, child in [(0x1400052c0, self.natural, self.child_natural),
                (0x140004680, self.map_component, self.child_map),
                (0x140003f60, self.child_model, self.child_model),
                (0x14000c460, self.child_growth, self.child_growth)]:
            def lookup(args, parent=parent, child=child):
                self.u64(args[1], parent if args[0] == self.owner else child)
                return args[1]
            e.hook_function(address, 'component lookup', lookup)
        def ground(args):
            x, y, _ = e.f32s(args[2], 3)
            e.write(args[1], struct.pack('<3f', x, y, 0.0))
            return 1
        e.hook_function(0x1400a48b0, 'flat ground query', ground)
        self.admit, self.density = True, True
        self.candidates, self.registered, self.constructed = [], [], False
        def admit(args):
            self.candidates.append((args[1], args[2]))
            return int(self.admit)
        e.hook_function(0x1400a4aa0, 'candidate admission', admit)
        e.hook_function(0x140126100, 'neighbor density gate', lambda a: int(self.density))
        for prefix in ['?SetMesh@MeshInstance@Graphics@@', '?SetTransform@Geometry@Graphics@@']:
            matches = [(k, v) for k, v in e.exports.items() if k.startswith(prefix)]
            assert len(matches) == 1, (prefix, matches)
            e.hook_function(matches[0][1], 'graphics binding', lambda a: 0)
        self.pre_setup, self.post_setup = None, None
        def construct(args):
            assert e.read(self.random, 2504) == self.pre_setup
            e.write(self.random, self.post_setup)
            self.constructed = True
            return self.child_owner
        e.hook_function(0x1400c8bf0, 'prepared model/growth constructor replay', construct)
        e.hook_function(0x1400c9d70, 'registration observation', lambda a: self.registered.append(a[2]) or 0)

    def u32(self, address, value): self.emu.write(address, struct.pack('<I', value))
    def u64(self, address, value): self.emu.write(address, struct.pack('<Q', value))
    def f32(self, address, value): self.emu.write(address, struct.pack('<f', value))

    def setup_at_candidate(self, seed, one_in, radius):
        e = self.emu
        e.call(e.exports['??0Random@System@@QEAA@I@Z'], self.random, seed)
        before = e.read(self.random, 2504)
        e.call(e.exports['?RandFloat@Random@System@@QEAAMXZ'], self.random)
        for _ in range(2): e.call(e.exports['?RandTol@Random@System@@QEAAHH@Z'], self.random, radius)
        self.pre_setup = e.read(self.random, 2504)
        e.call(0x140129f90, self.child_model)
        e.call(0x140125bc0, self.child_growth)
        self.post_setup = e.read(self.random, 2504)
        e.write(self.random, before)
        self.constructed = False; self.registered.clear(); self.candidates.clear()


def main(out_dir):
    os.makedirs(out_dir, exist_ok=True)
    h = Harness(); e = h.emu
    # The setup callback sizes the integer timer array and zeroes each new timer.
    e.call(0x1400567f0, h.component)
    timers = e.u64(h.component + 0x28)
    assert e.read(timers, 40) == bytes(40)
    attempts = []
    # Timer cases run separately so the child helper itself remains available for the RNG cases.
    timer_emu = Harness(); t = timer_emu.emu
    t.call(0x1400567f0, timer_emu.component)
    timer_array = t.u64(timer_emu.component + 0x28)
    t.hook_function(0x1400560e0, 'timed attempt observation', lambda a: attempts.append((a[2], a[3])) or 0)
    dt_ptr = t.malloc(8)
    rows = []
    periods = [1.0, 0.5, 1.337, 3.25, 0.125]
    for i, value in enumerate(periods): timer_emu.f32(timer_emu.growths[i] + 0x28, value)
    for growth, initial, delta in itertools.product((0.0, 0.1, 1.1, 2.1, 4.1, 10.0, 10.1),
            (0, 37_499_999, 150_000_000, 300_000_000), (0, 1, 37_500_000, 300_000_000, 901_000_000)):
        attempts.clear()
        timer_emu.f32(timer_emu.natural + 0x28, growth)
        t.write(timer_array, struct.pack('<5q', *([initial] * 5)))
        t.write(dt_ptr, struct.pack('<q', delta))
        t.call(0x140056430, timer_emu.component, dt_ptr)
        assert all(flag == 0 for _, flag in attempts)
        remaining = struct.unpack('<5q', t.read(timer_array, 40))
        rows.append('\t'.join(map(str, [growth, initial, delta, *remaining,
            ','.join(str(index) for index, _ in attempts)])))
    with open(os.path.join(out_dir, 'timers.tsv'), 'w') as output: output.write('\n'.join(rows) + '\n')
    # Native default natural-resource quantity/growth is 1 even without GrowthComponent.
    natural = e.call(0x140036eb0)
    assert e.u32(natural + 0x28) == 0x3f800000
    children = []
    for seed, one_in, radius, parent, admission, density, random_growth, disabled in itertools.product(
            range(1, 11), (1, 2, 8), (0, 4), (0, 8, 15), (0, 1), (0, 1), (0, 1), (0, 1)):
        g = h.growths[0]
        for offset, value in [(0x20, 4.0), (0x24, 1.0), (0x28, 1.0)]: h.f32(g + offset, value)
        h.u32(g + 0x48, one_in); h.u32(g + 0x4c, radius); h.u32(g + 0x50, 2)
        e.write(h.map_component + 0x28, struct.pack('<hh', parent, parent))
        h.u32(h.game + 0xd0, disabled << 10)
        h.admit, h.density = bool(admission), bool(density)
        h.setup_at_candidate(seed, one_in, radius)
        e.call(0x1400560e0, h.component, h.description, 0, random_growth)
        created = int(h.constructed)
        coords = struct.unpack('<hh', e.read(h.child_map + 0x28, 4)) if created else (0, 0)
        model = struct.unpack('<hh', e.read(h.child_model + 0x164, 4)) if created else (0, 0)
        values = [e.u32(h.child_natural + 0x28), e.u32(h.child_growth + 0x4c),
            e.u64(h.child_growth + 0x40), *struct.unpack('<4I', e.read(h.child_model + 0x134, 16)),
            *struct.unpack('<3I', e.read(h.child_model + 0x158, 12)), e.u32(h.child_model + 0x168)] if created else [0] * 11
        next_word = e.call(e.exports['?Rand@Random@System@@QEAAIXZ'], h.random)
        children.append('\t'.join(map(str, [seed, one_in, radius, parent, admission, density,
            random_growth, disabled, created, *coords, *model, *values,
            h.registered[0] if created else 0, next_word])))
    with open(os.path.join(out_dir, 'children.tsv'), 'w') as output: output.write('\n'.join(children) + '\n')
    print(f'wrote {len(rows)} timer and {len(children)} child/RNG cases')


if __name__ == '__main__': main(sys.argv[1])
