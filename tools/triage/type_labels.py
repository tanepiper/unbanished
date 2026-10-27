#!/usr/bin/env python3
"""Label the class registrations and factory callbacks in docs/GAME-TYPE-REGISTRATIONS.tsv.

Each confirmed row of the table names a class's registration function (the static initialiser
that builds its `Object::BaseTypeInfo`), its allocating callback (`MemAlloc`, then the
constructor) and its in-place callback (the constructor on memory it is given, as the Runtime's
`CreateInPlaceObject`). This writes them into labels/names.tsv as `Class::RegisterTypeInfo`,
`Class::Create` and `Class::CreateInPlace`, with the table as the evidence, between the two
marker lines below. Rows outside the markers are never touched.

Only addresses where a function starts are labelled (analysis/export/<binary>/functions.tsv):
37 in-place callbacks are labels Ghidra never made a function of (`LAB_140030b10`). An address
already labelled by hand outside the block keeps its label.

Usage: python3 tools/triage/type_labels.py [--check]
--check exits 1 if names.tsv would change. Run it again after
tools/schema/export_type_registrations.py.
"""
import csv
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
TABLE = os.path.join(ROOT, 'docs', 'GAME-TYPE-REGISTRATIONS.tsv')
NAMES = os.path.join(ROOT, 'labels', 'names.tsv')
FUNCTIONS = os.path.join(ROOT, 'analysis', 'export', 'Application-steam-x64.exe', 'functions.tsv')
BEGIN = '# Begin: class registrations and factory callbacks (tools/triage/type_labels.py; do not edit)'
END = '# End: class registrations and factory callbacks'
COLUMNS = (('registration_function', 'RegisterTypeInfo'), ('allocating_callback', 'Create'),
           ('in_place_callback', 'CreateInPlace'))
ADDRESS = re.compile(r'\b(?:FUN_|LAB_)?(14[0-9a-f]{7})\b')


def rows(starts):
    evidence = os.path.relpath(TABLE, ROOT)
    out = {}
    for row in csv.DictReader(open(TABLE, encoding='utf-8'), delimiter='\t'):
        if row['application_registration_status'] != 'confirmed':
            continue
        for column, role in COLUMNS:
            m = ADDRESS.search(row[column])
            if m and m.group(1) in starts:
                out.setdefault(m.group(1), f'{row["class_name"]}::{role}')
    return [f'{a}\t{name}\t{evidence}' for a, name in sorted(out.items())]


def main(argv):
    if not os.path.exists(FUNCTIONS):
        print(f'{FUNCTIONS} is missing: run tools/ghidra/export.sh first', file=sys.stderr)
        return 2
    starts = {line.split('\t')[0] for line in open(FUNCTIONS, encoding='utf-8', errors='replace')}
    old = open(NAMES, encoding='utf-8').read().split('\n')
    if BEGIN in old:
        i, j = old.index(BEGIN), old.index(END)
        before, after = old[:i], old[j + 1:]
    else:
        before, after = [line for line in old if line], ['']
    hand = {line.split('\t')[0] for line in before + after if line and not line.startswith('#')}
    block = [r for r in rows(starts) if r.split('\t')[0] not in hand]
    text = '\n'.join(before + [BEGIN] + block + [END] + after)
    if '--check' in argv:
        if text != '\n'.join(old):
            print('labels/names.tsv is out of date: run tools/triage/type_labels.py', file=sys.stderr)
            return 1
        return 0
    open(NAMES, 'w', encoding='utf-8').write(text)
    print(f'{len(block)} labels from {os.path.relpath(TABLE, ROOT)}', file=sys.stderr)
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
