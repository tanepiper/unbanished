"""Resolve a registered class (e.g. a UI controller) to its factory, constructor, vtable and
slot functions, from the local Ghidra export.

Usage: python3 tools/triage/controller.py ClassName [slots]
"""
import re
import struct
import sys

EXE = 'game-data/original/Application-steam-x64.exe'
DECOMP = 'analysis/export/Application-steam-x64.exe/decomp.c'


def sections(d):
    pe = struct.unpack_from('<I', d, 0x3c)[0]
    n = struct.unpack_from('<H', d, pe + 6)[0]
    opt = struct.unpack_from('<H', d, pe + 20)[0]
    out = []
    for i in range(n):
        vs, va, rs, rp = struct.unpack_from('<IIII', d, pe + 24 + opt + i * 40 + 8)
        out.append((va, vs, rp))
    return out


def main(name, slots=24):
    text = open(DECOMP).read()
    d = open(EXE, 'rb').read()
    secs = sections(d)

    def q(a):
        r = a - 0x140000000
        for va, vs, rp in secs:
            if va <= r < va + vs:
                return struct.unpack_from('<Q', d, r - va + rp)[0]

    def fn(addr):
        m = re.search(r'// ==== %x [^\n]*\n(.*?)(?=\n// ==== )' % addr, text, re.S)
        return m.group(1) if m else ''

    m = re.search(r'"%s","([\w:]+)",\(char \*\)0x0,\s*(FUN_([0-9a-f]+))' % name, text)
    if not m:
        print('no registration found')
        return
    factory = int(m.group(3), 16)
    print(f'{name} : {m.group(1)}  factory {factory:#x}')
    body = fn(factory)
    vts = re.findall(r'&PTR_FUN_([0-9a-f]+)', body)
    if vts:
        print(f'constructor inlined in factory, vtables {vts}')
    else:
        ctor = [int(c, 16) for c in re.findall(r'FUN_([0-9a-f]+)\(', body) if int(c, 16) != factory]
        if not ctor:
            print('no constructor')
            return
        vts = re.findall(r'&PTR_FUN_([0-9a-f]+)', fn(ctor[0]))
        print(f'constructor {ctor[0]:#x}, vtables {vts}')
    vt = int(vts[-1], 16)
    for i in range(slots):
        print(f'  +{i * 8:#05x} {q(vt + i * 8):#x}')


if __name__ == '__main__':
    main(sys.argv[1], int(sys.argv[2]) if len(sys.argv) > 2 else 24)
