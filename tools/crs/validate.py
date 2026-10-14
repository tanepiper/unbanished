#!/usr/bin/env python3
"""Decode every .crs under a directory and report per-class success.

Usage: python3 -m tools.crs.validate game-data/extracted [ClassName ...]
"""
import collections
import os
import sys

from . import types  # noqa: F401  (registers decoders)
from .crs import DECODERS, FormatError, read_resource


def main(root, only=()):
    ok, failed, skipped = collections.Counter(), collections.defaultdict(list), collections.Counter()
    for dirpath, _, files in os.walk(root):
        for f in files:
            if not f.endswith('.crs'):
                continue
            path = os.path.join(dirpath, f)
            data = open(path, 'rb').read()
            try:
                res = read_resource(data, decode=False)
            except FormatError as e:
                failed['<container>'].append((path, str(e)))
                continue
            name = res['class']
            if name is None or (only and name not in only):
                skipped[name or hex(res['type_id'])] += 1
                continue
            try:
                read_resource(data)
                ok[name] += 1
            except (FormatError, Exception) as e:  # report every decoder failure, keep going
                failed[name].append((os.path.relpath(path, root), f'{type(e).__name__}: {e}'))
    for name in sorted(set(ok) | set(failed)):
        print(f'{name:28} ok={ok[name]:5} failed={len(failed[name])}')
        for p, err in failed[name][:3]:
            print(f'    {p}: {err}')
    print(f'undecoded classes: {len(skipped)} ({sum(skipped.values())} files)')
    return 1 if any(failed.values()) else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1], set(sys.argv[2:])))
