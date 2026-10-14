"""Disassemble D3D9 shader model 2/3 bytecode found inside a compiled resource (.crs).

Usage: python3 tools/shader/d3d9_disasm.py FILE.crs [INDEX]

Banished's VertexProgram / PixelProgram resources carry several D3D9 variants followed by D3D11
DXBC blobs. This scans for D3D9 version tokens, prints each program's constant table (CTAB) and
its instructions in the usual `op dst, src...` form. Output is for analysis only (the shaders are
the game's; keep any dumps in git-ignored folders).
"""
import struct
import sys

OPS = {
    0: 'nop', 1: 'mov', 2: 'add', 3: 'sub', 4: 'mad', 5: 'mul', 6: 'rcp', 7: 'rsq', 8: 'dp3',
    9: 'dp4', 10: 'min', 11: 'max', 12: 'slt', 13: 'sge', 14: 'exp', 15: 'log', 16: 'lit',
    17: 'dst', 18: 'lrp', 19: 'frc', 20: 'm4x4', 21: 'm4x3', 22: 'm3x4', 23: 'm3x3', 24: 'm3x2',
    25: 'call', 26: 'callnz', 27: 'loop', 28: 'ret', 29: 'endloop', 30: 'label', 31: 'dcl',
    32: 'pow', 33: 'crs', 34: 'sgn', 35: 'abs', 36: 'nrm', 37: 'sincos', 38: 'rep', 39: 'endrep',
    40: 'if', 41: 'ifc', 42: 'else', 43: 'endif', 44: 'break', 45: 'breakc', 46: 'mova',
    47: 'defb', 48: 'defi', 64: 'texcoord', 65: 'texkill', 66: 'tex', 67: 'texbem',
    68: 'texbeml', 69: 'texreg2ar', 70: 'texreg2gb', 71: 'texm3x2pad', 72: 'texm3x2tex',
    73: 'texm3x3pad', 74: 'texm3x3tex', 76: 'texm3x3spec', 77: 'texm3x3vspec', 78: 'expp',
    79: 'logp', 80: 'cnd', 81: 'def', 82: 'texreg2rgb', 83: 'texdp3tex', 84: 'texm3x2depth',
    85: 'texdp3', 86: 'texm3x3', 87: 'texdepth', 88: 'cmp', 89: 'bem', 90: 'dp2add', 91: 'dsx',
    92: 'dsy', 93: 'texldd', 94: 'setp', 95: 'texldl', 96: 'breakp',
}
# Number of source operands (after the destination, if any).
NO_DEST = {'nop', 'call', 'callnz', 'loop', 'ret', 'endloop', 'label', 'rep', 'endrep', 'if',
           'ifc', 'else', 'endif', 'break', 'breakc', 'texkill', 'breakp'}
REG = {0: 'r', 1: 'v', 2: 'c', 3: 't', 4: 'oPos', 5: 'oD', 6: 'o', 7: 'i', 8: 'oC', 9: 'oDepth',
       10: 's', 11: 'c', 12: 'c', 13: 'c', 14: 'b', 15: 'aL', 17: 'vMisc', 18: 'l', 19: 'p'}
USAGE = ['position', 'blendweight', 'blendindices', 'normal', 'psize', 'texcoord', 'tangent',
         'binormal', 'tessfactor', 'positiont', 'color', 'fog', 'depth', 'sample']
SRC_MOD = {0: '{}', 1: '-{}', 2: '{}_bias', 3: '-{}_bias', 4: '{}_bx2', 5: '-{}_bx2',
           6: '1-{}', 7: '{}_x2', 8: '-{}_x2', 9: '{}_dz', 10: '{}_dw', 11: '{}_abs',
           12: '-{}_abs', 13: '!{}'}


def reg_name(tok, vs):
    t = ((tok >> 28) & 7) | ((tok >> 8) & 0x18)
    n = tok & 0x7ff
    if t == 3:
        return f'a{n}' if vs else f't{n}'
    if t == 4:
        return ['oPos', 'oFog', 'oPts'][n]
    if t == 6:
        return f'oT{n}' if not vs else f'o{n}'
    if t == 17:
        return ['vPos', 'vFace'][n]
    return f'{REG.get(t, f"?{t}")}{n}'


def dest(tok, vs):
    s = reg_name(tok, vs)
    mask = (tok >> 16) & 0xf
    if mask != 0xf:
        s += '.' + ''.join(c for i, c in enumerate('xyzw') if mask >> i & 1)
    mod = (tok >> 20) & 0xf
    pre = ('_sat' if mod & 1 else '') + ('_pp' if mod & 2 else '') + ('_centroid' if mod & 4 else '')
    return s, pre


def src(tok, vs, rel=None):
    s = reg_name(tok, vs)
    if rel is not None:
        s += f'[{reg_name(rel, vs)}]'
    sw = (tok >> 16) & 0xff
    if sw != 0xe4:
        comps = ''.join('xyzw'[(sw >> (2 * i)) & 3] for i in range(4))
        s += '.' + (comps[0] if len(set(comps)) == 1 else comps)
    return SRC_MOD.get((tok >> 24) & 0xf, '{}?').format(s)


def ctab(words):
    data = struct.pack(f'<{len(words)}I', *words)
    if data[:4] != b'CTAB':
        return None
    d = data[4:]
    _size, _creator, _ver, count, info, _flags, _target = struct.unpack_from('<7I', d)
    out = []
    for i in range(count):
        name, rset, ridx, rcount, _r, _type, _dv = struct.unpack_from('<IHHHHII', d, info + 20 * i)
        cname = d[name:d.index(b'\0', name)].decode()
        out.append((['bool', 'int4', 'float4', 'sampler'][rset], ridx, rcount, cname))
    return out


def disasm(words, at):
    ver = words[at]
    vs = (ver >> 16) == 0xfffe
    major = (ver >> 8) & 0xff
    lines = [f'{"vs" if vs else "ps"}_{major}_{ver & 0xff}']
    i = at + 1
    while i < len(words):
        tok = words[i]
        op = tok & 0xffff
        if tok == 0x0000ffff:
            lines.append('end')
            return lines, i + 1
        if op == 0xfffe:
            n = (tok >> 16) & 0x7fff
            table = ctab(words[i + 1:i + 1 + n])
            if table:
                for rset, ridx, rcount, cname in table:
                    prefix = {'sampler': 's', 'bool': 'b', 'int4': 'i'}.get(rset, 'c')
                    lines.append(f'// {prefix}{ridx}..+{rcount}  {cname}')
            i += 1 + n
            continue
        length = (tok >> 24) & 0xf
        name = OPS.get(op, f'op{op}')
        args = words[i + 1:i + 1 + length]
        if name == 'dcl':
            d, pre = dest(args[1], vs)
            t = ((args[1] >> 28) & 7) | ((args[1] >> 8) & 0x18)
            if t == 10:
                kind = {2: '2d', 3: 'cube', 4: 'volume'}.get((args[0] >> 27) & 0xf, '?')
                lines.append(f'dcl_{kind} {d}')
            elif not vs and major < 3:
                lines.append(f'dcl{pre} {d}')
            else:
                usage = USAGE[args[0] & 0x1f] if (args[0] & 0x1f) < len(USAGE) else '?'
                idx = (args[0] >> 16) & 0xf
                lines.append(f'dcl_{usage}{idx or ""}{pre} {d}')
        elif name == 'def':
            d, _ = dest(args[0], vs)
            f = struct.unpack('<4f', struct.pack('<4I', *args[1:5]))
            lines.append(f'def {d}, ' + ', '.join(f'{v:.9g}' for v in f))
        elif name in ('defi', 'defb'):
            d, _ = dest(args[0], vs)
            lines.append(f'{name} {d}, ' + ', '.join(str(struct.unpack('<i', struct.pack('<I', v))[0]) for v in args[1:]))
        else:
            ops = []
            k = 0
            first = name not in NO_DEST
            pre = ''
            while k < len(args):
                t = args[k]
                rel = None
                if t & (1 << 13) and major >= 3 and k + 1 < len(args):
                    rel = args[k + 1]
                if first:
                    d, pre = dest(t, vs)
                    ops.append(d)
                    first = False
                else:
                    ops.append(src(t, vs, rel))
                k += 2 if rel is not None else 1
            ctrl = (tok >> 16) & 0xff
            suffix = ''
            if name in ('ifc', 'breakc', 'setp'):
                suffix = '_' + ['?', 'gt', 'eq', 'ge', 'lt', 'ne', 'le'][ctrl & 7]
            lines.append(f'{name}{suffix}{pre} ' + ', '.join(ops))
        i += 1 + length
    return lines, i


def programs(data):
    words = struct.unpack(f'<{len(data) // 4}I', data[: len(data) // 4 * 4])
    starts = [i for i, w in enumerate(words)
              if (w >> 16) in (0xfffe, 0xffff) and (w & 0xffff) in (0x0200, 0x0201, 0x0300)
              and i + 1 < len(words) and (words[i + 1] & 0xffff) == 0xfffe]
    return words, starts


def main(path, which=None):
    for align in range(4):
        data = open(path, 'rb').read()[align:]
        words, starts = programs(data)
        for n, at in enumerate(starts):
            if which is not None and n != int(which):
                continue
            lines, _ = disasm(words, at)
            print(f'==== program {n} (offset {align + 4 * at:#x})')
            print('\n'.join(lines))
        if starts:
            return


if __name__ == '__main__':
    main(*sys.argv[1:])
