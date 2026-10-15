"""Original Graphics::SunLight::GetSunCamera 1801098a0 over random main cameras and sun directions.

The main camera's frustum is built by the original Volume::Frustum::Frustum(near, far, hfov,
vfov) 1800fca70; Frustum::GetCorners, Transform3::FromLookAt, Camera::Camera/SetFrustum and the
CRT maths run unchanged. Only the Device's "D3D conventions" virtual (+0x38) is a stub returning 1.
Each row: inputs (eye, right, forward, up, hfov, vfov, light) then the sun camera's rotation rows,
position and frustum half-width, all as f32 bit patterns.
Usage: .venv/bin/python tools/emu/sun_camera.py game-data/derived/emu/sun_camera.tsv
"""
import math
import random
import struct
import sys
sys.path.insert(0, 'tools/emu')
from pe_emu import Emu

DEVICE = 0x1802027a8
CONFIGURATION = 0x1802027a0


def f32(v):
    return struct.unpack('<f', struct.pack('<f', v))[0]


def norm(v):
    l = math.sqrt(sum(x * x for x in v))
    return [f32(x / l) for x in v]


def cross(a, b):
    return [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]


def main(out):
    e = Emu('game-data/original/Application-steam-x64.exe', 'game-data/original/Runtime-steam-x64.dll')
    q = lambda p, v: e.write(p, struct.pack('<Q', v))
    i = lambda p, v: e.write(p, struct.pack('<i', v))
    floats = lambda p, v: e.write(p, struct.pack('<' + 'f' * len(v), *v))
    bits = lambda p, n: [str(x) for x in struct.unpack('<%dI' % n, e.read(p, 4 * n))]
    # Device: +0x100 -> object whose vtable +0x38 says "D3D conventions" (true on DX9).
    d3d = e._new_stub('Device::+0x38 (D3D conventions)', lambda a: 1)
    device, video, vtable = e.malloc(0x200), e.malloc(0x10), e.malloc(0x100)
    q(vtable + 0x38, d3d); q(video, vtable); q(device + 0x100, video); q(DEVICE, device)
    config = e.malloc(0x80); i(config + 0x2c, 1); q(CONFIGURATION, config)
    shadow = e.malloc(0x80); i(shadow + 0x20, 2048); i(shadow + 0x24, 2048); i(shadow + 0x34, 0x40)
    sun = e.malloc(0x108); q(sun + 0xf8, shadow)
    entry = e.malloc(0x1f0); renderer = e.malloc(0x50); q(renderer + 0x10, entry)
    scalars = e.malloc(16); camera = e.malloc(0x100)
    rng = random.Random(4127)
    rows = []
    for case in range(400):
        yaw, pitch = rng.uniform(0, 360), rng.uniform(-88, -20)
        if case % 10 == 0:
            pitch = rng.uniform(-15, 5)  # rays that miss the ground plane
        distance = rng.uniform(20, 130)
        aspect = rng.choice([4 / 3, 16 / 10, 16 / 9])
        vfov = 40.0; hfov = f32(aspect * vfov)
        y, p = math.radians(yaw), math.radians(pitch)
        forward = norm([-math.cos(p) * math.sin(y), math.cos(p) * math.cos(y), math.sin(p)])
        right = norm(cross(forward, [0, 0, 1]))
        up = norm(cross(right, forward))
        target = [rng.uniform(-60, 60), rng.uniform(-60, 60), 0.0]
        eye = [f32(t - f * distance) for t, f in zip(target, forward)]
        ly, lp = math.radians(rng.uniform(0, 360)), math.radians(rng.uniform(-80, -15))
        light = norm([-math.cos(lp) * math.sin(ly), math.cos(lp) * math.cos(ly), math.sin(lp)])
        floats(scalars, [2.0, 512.0, hfov, vfov])
        e.call(0x1800fca70, entry + 8, scalars, scalars + 4, scalars + 8, scalars + 12)
        floats(entry + 0x6c, right + forward + up + eye)
        floats(sun + 0xc0, light)
        e.call(0x1801098a0, sun, camera, renderer)
        inputs = eye + right + forward + up + [hfov, vfov] + light
        row = [','.join(str(struct.unpack('<I', struct.pack('<f', v))[0]) for v in inputs)]
        # Transform3 at +0x64: rotation rows, then translation; frustum +0x2c = -radius.
        row.append(','.join(bits(camera + 0x64, 12)))
        row.append(bits(camera + 0x2c, 1)[0])
        rows.append('\t'.join(row))
    open(out, 'w').write('\n'.join(rows) + '\n')
    print('wrote %d GetSunCamera cases' % len(rows))


if __name__ == '__main__':
    main(*sys.argv[1:])
