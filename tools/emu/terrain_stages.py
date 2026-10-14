"""Ground truth for the terrain generator (FUN_1400d4860) stage by stage.

Usage: .venv/bin/python tools/emu/terrain_stages.py OUT_DIR SEED STRUCT_WORDS(17 comma-separated)
Writes grid_N.f32 after each stage plus paths.bin / points.bin / lakes.bin, then runs the
packed surface-map builder (FUN_1400d4910) and writes surface.u32. Set SURFACE=0 to skip it.
"""
import os
import struct
import sys
import time

sys.path.insert(0, os.path.dirname(__file__))
from pe_emu import Emu  # noqa: E402

STAGES = [
    (1, ['1400d3aa0']),
    (2, ['1400d2560', '1400d2620', '1400d2430']),
    (3, ['1400d2930', '1400d3de0']),
    (4, ['1400d1550']),
    (5, ['1400d4120', '1400d1820', '1400d1820']),
]


def new_emu():
    return Emu('game-data/original/Application-steam-x64.exe', 'game-data/original/Runtime-steam-x64.dll')


def build_context(emu, seed, words):
    """A generation context as FUN_1400b5700 prepares it for FUN_1400d4860."""
    # Progress object written by FUN_1400d4860 (DAT_1401cbdd8 + 0x24).
    progress = emu.malloc(0x100)
    emu.write(0x1401cbdd8, struct.pack('<Q', progress))
    ctx = emu.malloc(0xa40)
    record = emu.malloc(0x44)
    emu.write(record, struct.pack('<17i', *words))
    emu.write(ctx + 0xa28, struct.pack('<Q', record))
    emu.call(emu.exports['??0Random@System@@QEAA@I@Z'], ctx + 0x58, seed)
    emu.write(ctx, struct.pack('<i', words[0] * 2))  # FUN_1400d4860 sets *ctx = n * 2
    return ctx


def generate(emu, seed, words, after_stage=None):
    ctx = build_context(emu, seed, words)
    for stage, funcs in STAGES:
        for f in funcs:
            emu.call(int(f, 16), ctx)
        if after_stage:
            after_stage(stage, ctx)
    return ctx


def main(out, seed, words):
    os.makedirs(out, exist_ok=True)
    emu = new_emu()
    ctx = build_context(emu, seed, words)
    for stage, funcs in STAGES:
        for f in funcs:
            t = time.time()
            emu.call(int(f, 16), ctx)
            print(f'stage {stage} {f}: {time.time() - t:.1f}s', flush=True)
        size = emu.i32(ctx) + 1
        ptr, count = emu.u64(ctx + 8), emu.i32(ctx + 0x10)
        assert count == size * size, (count, size)
        with open(os.path.join(out, f'grid_{stage}.f32'), 'wb') as fh:
            fh.write(emu.read(ptr, 4 * count))
    def dump_vec(off, elem, name):
        ptr, count = emu.u64(ctx + off), emu.i32(ctx + off + 8)
        with open(os.path.join(out, name), 'wb') as fh:
            fh.write(struct.pack('<i', count))
            if elem == 'paths':
                for i in range(count):
                    p, c = emu.u64(ptr + 16 * i), emu.i32(ptr + 16 * i + 8)
                    fh.write(struct.pack('<i', c) + emu.read(p, 28 * c))
            else:
                fh.write(emu.read(ptr, elem * count))
        print(name, count)
    dump_vec(0x18, 'paths', 'paths.bin')
    dump_vec(0x28, 12, 'vec28.bin')
    dump_vec(0x38, 12, 'points.bin')
    dump_vec(0x48, 12, 'lakes.bin')
    with open(os.path.join(out, 'random.bin'), 'wb') as fh:
        fh.write(emu.read(ctx + 0x58, 2504))
    if os.environ.get('SURFACE', '1') != '0':
        t = time.time()
        ptr = emu.call(0x1400d4910, ctx)
        n = emu.i32(ctx)
        with open(os.path.join(out, 'surface.u32'), 'wb') as fh:
            fh.write(emu.read(ptr, 4 * n * n))
        print(f'surface FUN_1400d4910: {time.time() - t:.1f}s', flush=True)


if __name__ == '__main__':
    main(sys.argv[1], int(sys.argv[2]), [int(v) for v in sys.argv[3].split(',')])
