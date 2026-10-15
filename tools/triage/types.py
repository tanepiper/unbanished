"""Registered classes of the original executable: registration, factory, size, constructor,
vtable and slots (with the known meaning of component and action slots).

  python3 tools/triage/types.py class CitizenAIComponent   # everything about one class
  python3 tools/triage/types.py list --parent Action        # classes by parent
  python3 tools/triage/types.py owner 0x1401148b0           # which class's vtable holds a function
  python3 tools/triage/types.py vtable 0x140168790          # raw slots of any vtable

Type ids are `CRC32(UTF-16LE name)` (tools/schema/typeids.py); the id is printed for matching
against resource headers.
"""
import argparse
import os
import sys
import zlib

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from ghidra_index import ACTION_SLOTS, COMPONENT_SLOTS, DIALOG_SLOTS, Index, norm  # noqa: E402


def type_id(name):
    return zlib.crc32(name.encode('utf-16-le')) & 0xffffffff


def slot_meanings(ix, name, parent):
    if parent == 'Component' or name.endswith('Component'):
        return COMPONENT_SLOTS
    if parent == 'Action':
        return ACTION_SLOTS
    if parent == 'UI::DialogController':
        return DIALOG_SLOTS
    return {0x18: 'serialize'} if parent in ('Description', 'Object::External') else {}


def print_slots(ix, va, meanings, limit=48):
    for i, s in enumerate(ix.vtable(int(va, 16), limit)):
        off = 8 * i
        meaning = meanings.get(off, '')
        print(f'  +{off:#05x} {ix.describe(s)}{"  — " + meaning if meaning else ""}')


def cmd_class(ix, args):
    info = ix.class_info(args.name)
    if not info:
        close = [v[0] for v in ix.classes.values() if args.name.lower() in v[0].lower()]
        print(f'no class {args.name!r}; similar: {", ".join(sorted(close)[:20])}')
        return 1
    dat, parent, factory = info
    print(f'{args.name} : {parent}   type id {type_id(args.name):#010x}   type info DAT_{dat}')
    if not factory:
        print('  no factory (abstract or registered without one)')
        return
    size, ctor, vt = ix.constructor(factory)
    print(f'  factory {factory}  allocates {size:#x} bytes' if size else f'  factory {factory}')
    if ctor:
        print(f'  constructor {ix.describe(ctor)}')
    children = sorted(v[0] for v in ix.classes.values() if v[1] == args.name)
    if children:
        print(f'  subclasses: {", ".join(children)}')
    if vt:
        print(f'  vtable {vt}:')
        print_slots(ix, vt, slot_meanings(ix, args.name, parent), args.slots)


def cmd_list(ix, args):
    rows = sorted((v[1], v[0]) for v in ix.classes.values()
                  if not args.parent or v[1] == args.parent)
    for parent, name in rows:
        print(f'{name:40s} : {parent}')
    print(f'{len(rows)} classes')


def cmd_owner(ix, args):
    fn = norm(args.address)
    hits = []
    for vt, name in sorted(ix.vtable_classes().items(), key=lambda kv: kv[1]):
        for i, s in enumerate(ix.vtable(int(vt, 16))):
            if s == fn:
                hits.append(f'{name}::slot+{8 * i:#x} (vtable {vt})')
    print(ix.describe(fn))
    for h in hits or ['  not in any registered class vtable']:
        print('  ', h)


def cmd_vtable(ix, args):
    va = norm(args.address)
    name = ix.vtable_classes().get(va)
    print(f'vtable {va}' + (f' = {name}' if name else ''))
    parent = ix.class_info(name)[1] if name else ''
    print_slots(ix, va, slot_meanings(ix, name or '', parent), args.slots)


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    p.add_argument('--binary', default='Application-steam-x64.exe')
    sub = p.add_subparsers(dest='cmd', required=True)
    s = sub.add_parser('class')
    s.add_argument('name')
    s.add_argument('--slots', type=int, default=24)
    s = sub.add_parser('list')
    s.add_argument('--parent')
    s = sub.add_parser('owner')
    s.add_argument('address')
    s = sub.add_parser('vtable')
    s.add_argument('address')
    s.add_argument('--slots', type=int, default=24)
    args = p.parse_args()
    ix = Index(args.binary)
    return {'class': cmd_class, 'list': cmd_list, 'owner': cmd_owner,
            'vtable': cmd_vtable}[args.cmd](ix, args)


if __name__ == '__main__':
    sys.exit(main())
