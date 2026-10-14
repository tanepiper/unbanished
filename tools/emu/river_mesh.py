"""Ground truth for the river strip builder (FUN_1400d4fd0).

Usage: .venv/bin/python tools/emu/river_mesh.py OUT_DIR SEED STRUCT_WORDS(17 comma-separated)
Generates the map (as terrain_stages.py does), then expands every path record and writes
OUT_DIR/rivers.bin: u32 path count, then per path u32 vertex count, vertices (4 f32 each),
u32 index count, indices (u16 each).
"""
import os
import struct
import sys
import time

sys.path.insert(0, os.path.dirname(__file__))
from terrain_stages import generate, new_emu  # noqa: E402


def main(out, seed, words):
    os.makedirs(out, exist_ok=True)
    emu = new_emu()
    t = time.time()
    ctx = generate(emu, seed, words)
    print(f'generated in {time.time() - t:.1f}s', flush=True)
    paths = emu.i32(ctx + 0x20)
    verts = emu.malloc(16)
    idx = emu.malloc(16)
    with open(os.path.join(out, 'rivers.bin'), 'wb') as fh:
        fh.write(struct.pack('<I', paths))
        for p in range(paths):
            emu.write(verts, bytes(16))
            emu.write(idx, bytes(16))
            t = time.time()
            emu.call(0x1400d4fd0, ctx, p, verts, idx)
            vp, vc = emu.u64(verts), emu.i32(verts + 8)
            ip, ic = emu.u64(idx), emu.i32(idx + 8)
            fh.write(struct.pack('<I', vc) + emu.read(vp, 16 * vc))
            fh.write(struct.pack('<I', ic) + emu.read(ip, 2 * ic))
            print(f'path {p}: {vc} vertices, {ic} indices ({time.time() - t:.1f}s)', flush=True)


if __name__ == '__main__':
    main(sys.argv[1], int(sys.argv[2]), [int(v) for v in sys.argv[3].split(',')])
