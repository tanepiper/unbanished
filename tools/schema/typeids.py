#!/usr/bin/env python3
"""Map resource type IDs to engine class names.

Object::Factory::Startup (Runtime 0x1800c2ea0) assigns each registered type
  typeId = CRC32(name encoded as UTF-16LE)
using the standard CRC-32 table (System::CRC::_crcTable).

Candidate names come from `_typeInfo` symbols in the Ghidra exports and from
`*Description` strings in the game executable. Observed IDs are read from the
headers of the extracted .crs files.

Usage: typeids.py analysis/export game-data/extracted > tools/schema/types.json
"""
import collections
import json
import os
import re
import struct
import sys
import zlib

MAGIC = 0x5412A91F


def type_id(name):
    return zlib.crc32(name.encode('utf-16-le'))


def candidate_names(export_dir):
    names = set()
    for binary in os.listdir(export_dir):
        base = os.path.join(export_dir, binary)
        for line in open(os.path.join(base, 'exports.txt'), encoding='utf-8', errors='replace'):
            sym = line.rstrip('\n').split('\t')[-1]
            m = re.match(r'^([A-Za-z_][\w:]*)::_typeInfo$', sym)
            if m:
                full = m.group(1)
                names.update({full, full.split('::')[-1]})
        for line in open(os.path.join(base, 'functions.tsv'), encoding='utf-8', errors='replace'):
            name = line.split('\t')[2]
            if '::' in name:
                cls = name.rsplit('::', 1)[0]
                names.update({cls, cls.split('::')[-1]})
        for line in open(os.path.join(base, 'strings.txt'), encoding='utf-8', errors='replace'):
            s = line.rstrip('\n').split('\t', 1)[-1]
            if re.fullmatch(r'[A-Za-z_][\w:]{2,80}', s):
                names.add(s)
    return names


def observed_ids(data_dir):
    seen = collections.defaultdict(list)
    for root, _, files in os.walk(data_dir):
        for f in files:
            path = os.path.join(root, f)
            with open(path, 'rb') as fh:
                head = fh.read(24)
            if len(head) == 24 and head[:4] == b'\xaa' * 4 and struct.unpack_from('<I', head, 12)[0] == MAGIC:
                tid, ver = struct.unpack_from('<II', head, 16)
                seen[tid].append((os.path.relpath(path, data_dir), ver))
    return seen


def main(export_dir, data_dir):
    lookup = {}
    for n in candidate_names(export_dir):
        lookup.setdefault(type_id(n), set()).add(n)
    seen = observed_ids(data_dir)
    table = {}
    for tid, files in sorted(seen.items(), key=lambda kv: -len(kv[1])):
        table[f'0x{tid:08x}'] = {
            'names': sorted(lookup.get(tid, [])),
            'count': len(files),
            'versions': sorted({v for _, v in files}),
            'example': files[0][0],
        }
    json.dump(table, sys.stdout, indent=1)
    named = sum(1 for v in table.values() if v['names'])
    print(f'{named}/{len(table)} observed type ids named', file=sys.stderr)


if __name__ == '__main__':
    main(*sys.argv[1:3])
