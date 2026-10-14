#!/usr/bin/env python3
"""Print a vtable's slots (offset -> function) from a PE32+ image and a Ghidra functions.tsv.

Usage: vtable.py IMAGE.dll functions.tsv 0xVTABLE_VA [slots]
"""
import struct
import sys


def sections(img):
    pe = struct.unpack_from('<I', img, 0x3C)[0]
    nsec = struct.unpack_from('<H', img, pe + 6)[0]
    opt = struct.unpack_from('<H', img, pe + 20)[0]
    base = struct.unpack_from('<Q', img, pe + 24 + 24)[0]
    out = []
    for i in range(nsec):
        s = pe + 24 + opt + 40 * i
        vsize, va, rsize, raw = struct.unpack_from('<IIII', img, s + 8)
        out.append((va, max(vsize, rsize), raw))
    return base, out


def va_to_off(base, secs, va):
    rva = va - base
    for sva, size, raw in secs:
        if sva <= rva < sva + size:
            return raw + rva - sva
    raise ValueError(hex(va))


def main(image, funcs, vtable, slots='64'):
    img = open(image, 'rb').read()
    base, secs = sections(img)
    names = {}
    for line in open(funcs, encoding='utf-8', errors='replace'):
        addr, _, name, *_ = line.rstrip('\n').split('\t')
        names[int(addr, 16)] = name
    off = va_to_off(base, secs, int(vtable, 16))
    for i in range(int(slots)):
        ptr, = struct.unpack_from('<Q', img, off + 8 * i)
        if ptr not in names:
            break
        print(f'+0x{8 * i:03x}  {ptr:x}  {names[ptr]}')


if __name__ == '__main__':
    main(*sys.argv[1:])
