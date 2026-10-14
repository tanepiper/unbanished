"""Decode Video::ImageBuffer levels to RGBA and write PNGs (no third-party dependencies)."""
import struct
import zlib


def _rgb565(c):
    r, g, b = (c >> 11) & 31, (c >> 5) & 63, c & 31
    return (r << 3 | r >> 2, g << 2 | g >> 4, b << 3 | b >> 2)


def _bc1_colors(c0, c1, alpha_block):
    p0, p1 = _rgb565(c0), _rgb565(c1)
    if c0 > c1 or alpha_block:
        p2 = tuple((2 * a + b) // 3 for a, b in zip(p0, p1))
        p3 = tuple((a + 2 * b) // 3 for a, b in zip(p0, p1))
        return [p0 + (255,), p1 + (255,), p2 + (255,), p3 + (255,)]
    p2 = tuple((a + b) // 2 for a, b in zip(p0, p1))
    return [p0 + (255,), p1 + (255,), p2 + (255,), (0, 0, 0, 0)]


def _bc3_alphas(a0, a1):
    if a0 > a1:
        return [a0, a1] + [((7 - i) * a0 + i * a1) // 7 for i in range(1, 7)]
    return [a0, a1] + [((5 - i) * a0 + i * a1) // 5 for i in range(1, 5)] + [0, 255]


def decode_rgba(fmt, width, height, data):
    """Return width*height*4 bytes of RGBA8 for one level."""
    out = bytearray(width * height * 4)
    if fmt == 'B8G8R8A8':
        for i in range(width * height):
            b, g, r, a = data[4 * i:4 * i + 4]
            out[4 * i:4 * i + 4] = bytes((r, g, b, a))
        return bytes(out)
    if fmt in ('R8', 'A8'):
        for i, v in enumerate(data[:width * height]):
            out[4 * i:4 * i + 4] = bytes((v, v, v, 255)) if fmt == 'R8' else bytes((255, 255, 255, v))
        return bytes(out)
    if fmt not in ('BC1', 'BC3'):
        raise ValueError(f'unsupported format {fmt}')
    size = 8 if fmt == 'BC1' else 16
    bw, bh = (width + 3) // 4, (height + 3) // 4
    for by in range(bh):
        for bx in range(bw):
            o = (by * bw + bx) * size
            alphas = None
            if fmt == 'BC3':
                alphas = _bc3_alphas(data[o], data[o + 1])
                abits = int.from_bytes(data[o + 2:o + 8], 'little')
                o += 8
            c0, c1, bits = struct.unpack_from('<HHI', data, o)
            colors = _bc1_colors(c0, c1, fmt == 'BC3')
            for py in range(4):
                y = by * 4 + py
                if y >= height:
                    break
                for px in range(4):
                    x = bx * 4 + px
                    if x >= width:
                        continue
                    k = py * 4 + px
                    r, g, b, a = colors[(bits >> (2 * k)) & 3]
                    if alphas:
                        a = alphas[(abits >> (3 * k)) & 7]
                    p = (y * width + x) * 4
                    out[p:p + 4] = bytes((r, g, b, a))
    return bytes(out)


def write_png(path, width, height, rgba):
    def chunk(tag, body):
        return struct.pack('>I', len(body)) + tag + body + struct.pack('>I', zlib.crc32(tag + body))
    rows = b''.join(b'\0' + rgba[y * width * 4:(y + 1) * width * 4] for y in range(height))
    png = (b'\x89PNG\r\n\x1a\n'
           + chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, 6, 0, 0, 0))
           + chunk(b'IDAT', zlib.compress(rows, 6))
           + chunk(b'IEND', b''))
    with open(path, 'wb') as fh:
        fh.write(png)
