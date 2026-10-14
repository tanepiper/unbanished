#!/usr/bin/env python3
"""Export ImageBuffer resources (top mip, first face) to PNG.

Usage: python3 -m tools.crs.export_textures game-data/extracted game-data/derived/textures [substring ...]
"""
import os
import sys

from . import types  # noqa: F401
from .crs import read_resource, type_id
from .texture import decode_rgba, write_png

IMAGE_BUFFER = type_id('ImageBuffer').to_bytes(4, 'little')


def main(src, dst, filters=()):
    count = 0
    for dirpath, _, files in os.walk(src):
        for f in sorted(files):
            rel = os.path.relpath(os.path.join(dirpath, f), src)
            if filters and not any(s.lower() in rel.lower() for s in filters):
                continue
            data = open(os.path.join(dirpath, f), 'rb').read()
            if data[16:20] != IMAGE_BUFFER:
                continue
            img = read_resource(data)['object']
            if not img['levels']:
                continue
            w, h = img['width'], img['height']
            out = os.path.join(dst, os.path.splitext(rel)[0] + '.png')
            os.makedirs(os.path.dirname(out), exist_ok=True)
            write_png(out, w, h, decode_rgba(img['format'], w, h, img['levels'][0]))
            count += 1
            print(f"{rel}: {w}x{h} {img['format']}")
    print(f'exported {count} textures to {dst}')


if __name__ == '__main__':
    main(sys.argv[1], sys.argv[2], sys.argv[3:])
