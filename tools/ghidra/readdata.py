#!/usr/bin/env python3
"""Read typed constants from a PE image at virtual addresses (for DAT_ values in Ghidra output).

Usage: readdata.py IMAGE TYPE VA [VA ...]   TYPE is f32, f64, i32, u32 or hex16
"""
import struct
import sys

sys.path.insert(0, __file__.rsplit('/', 1)[0])
from vtable import sections, va_to_off  # noqa: E402

FORMATS = {'f32': '<f', 'f64': '<d', 'i32': '<i', 'u32': '<I'}


def main(image, kind, *vas):
    img = open(image, 'rb').read()
    base, secs = sections(img)
    for va in vas:
        off = va_to_off(base, secs, int(va, 16))
        if kind == 'hex16':
            print(va, img[off:off + 16].hex(' '))
        else:
            fmt = FORMATS[kind]
            print(va, struct.unpack_from(fmt, img, off)[0])


if __name__ == '__main__':
    main(*sys.argv[1:])
