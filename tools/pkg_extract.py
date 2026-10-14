#!/usr/bin/env python3
"""Extract resources from Banished .pkg archives (Shining Rock engine, v3 package format).

Format notes (recovered from Runtime-steam-x64.dll, see docs/FINDINGS.md):

  package:
    u32  header      bit31 = extended header flag, bits0-30 = TOC offset
    ...  file data   each entry is a block stream (below), except .index stubs and .str audio
    u32  tocSize     at TOC offset
    ...  TOC         a block stream of tocSize bytes

  block stream (IO::CompressedStream):
    u32  blockCount
    blockCount x { u32 offset, u16 compSize (0 = stored), u16 rawSize }
    blocks, each LZF-compressed (System::Compression) to <= 32 KiB
    stream length must equal 4 + 8*n + sum(compSize or rawSize)

  TOC (decoded, IO::PackageFile::ReadPackfile):
    u32 fileCount, u32 version, u32 platform, u32 userVersion, u32 priority, u32 unknown
    u8  canOverride, u32 crc, u32 iconW, u32 iconH, iconW*iconH*4 bytes RGBA
    v>1: u32 previewSize + bytes
    v>2: name, author, description as (u32 len, bytes)
    fileCount x { u32 nameLen, name (NUL-terminated), u32 offset, u32 size }

Usage: pkg_extract.py OUT_DIR data0.pkg [data1.pkg ...]
Raw .str (xWMA music) entries are written unmodified with a .str extension.
"""
import collections
import os
import struct
import sys

BLOCK = 0x8000


def lzf_decompress(src):
    out = bytearray()
    i = 0
    while i < len(src):
        ctrl = src[i]
        i += 1
        if ctrl < 0x20:
            n = ctrl + 1
            out += src[i:i + n]
            i += n
        else:
            n = ctrl >> 5
            if n == 7:
                n += src[i]
                i += 1
            ref = len(out) - ((ctrl & 0x1F) << 8) - src[i] - 1
            i += 1
            if ref < 0:
                raise ValueError('LZF back-reference before start of block')
            for k in range(n + 2):
                out.append(out[ref + k])
    if len(out) > BLOCK:
        raise ValueError('LZF block exceeds 32 KiB')
    return bytes(out)


def read_block_stream(buf):
    """Decode an IO::CompressedStream. Raises ValueError if the layout check fails."""
    if len(buf) < 4:
        raise ValueError('stream too short')
    n, = struct.unpack_from('<I', buf, 0)
    if 4 + 8 * n > len(buf):
        raise ValueError('block table exceeds stream')
    blocks = [struct.unpack_from('<IHH', buf, 4 + 8 * k) for k in range(n)]
    if 4 + 8 * n + sum(c or r for _, c, r in blocks) != len(buf):
        raise ValueError('stream size mismatch')
    out = bytearray()
    for off, comp, raw in blocks:
        out += lzf_decompress(buf[off:off + comp]) if comp else buf[off:off + raw]
    return bytes(out)


class Reader:
    def __init__(self, data):
        self.data, self.pos = data, 0

    def u32(self):
        v, = struct.unpack_from('<I', self.data, self.pos)
        self.pos += 4
        return v

    def take(self, n):
        b = self.data[self.pos:self.pos + n]
        self.pos += n
        return b

    def cstr(self, n):
        return self.take(n).split(b'\0')[0].decode('latin-1')


def read_package(path):
    data = open(path, 'rb').read()
    head, = struct.unpack_from('<I', data, 0)
    if not head >> 31:
        raise ValueError(f'{path}: legacy package header (bit31 clear) is not supported')
    toc_at = head & 0x7FFFFFFF
    toc_size, = struct.unpack_from('<I', data, toc_at)
    r = Reader(read_block_stream(data[toc_at + 4:toc_at + 4 + toc_size]))

    count, version = r.u32(), r.u32()
    if version < 3:
        raise ValueError(f'{path}: package version {version} (UTF-16 names) is not supported')
    info = dict(version=version, platform=r.u32(), user_version=r.u32(), priority=r.u32(), unknown=r.u32())
    info['can_override'] = r.take(1)[0]
    info['crc'] = r.u32()
    w, h = r.u32(), r.u32()
    info['icon'] = (w, h, r.take(w * h * 4))
    info['preview'] = r.take(r.u32())
    info['name'], info['author'], info['description'] = (r.cstr(r.u32()) for _ in range(3))

    files = []
    for _ in range(count):
        name = r.cstr(r.u32())
        files.append((name, r.u32(), r.u32()))
    if r.pos != len(r.data):
        raise ValueError(f'{path}: {len(r.data) - r.pos} unparsed TOC bytes')
    return data, info, files


def main(argv):
    if len(argv) < 3:
        sys.exit(__doc__)
    out_dir = argv[1]
    stats = collections.Counter()
    failures = []
    for path in argv[2:]:
        data, info, files = read_package(path)
        print(f"{os.path.basename(path)}: '{info['name']}' priority={info['priority']} files={len(files)}")
        for name, off, size in files:
            if name.endswith('.index'):
                continue
            raw = data[off:off + size]
            if name.endswith('.str'):
                payload, kind = raw, 'str'
            else:
                try:
                    payload, kind = read_block_stream(raw), 'crs'
                except ValueError as e:
                    failures.append((name, str(e)))
                    continue
            rel = name.replace('\\', '/').replace(':', '__')
            dst = os.path.join(out_dir, rel)
            os.makedirs(os.path.dirname(dst), exist_ok=True)
            with open(dst, 'wb') as fh:
                fh.write(payload)
            stats[kind] += 1
    print('extracted:', dict(stats), 'failed:', len(failures))
    for name, err in failures[:20]:
        print('  ', name, '-', err)
    return 1 if failures else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
