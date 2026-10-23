"""Shared index over the local Ghidra exports and original binaries, for the triage tools.

Knows, per binary: every function's size and body text, callers/callees, string literals,
global references, registered class types (`BaseTypeInfo` registrations: name, parent,
factory), constructors and vtables (with slots), typed constant reads, project mentions
("traced") and analyst labels from `labels/names.tsv`, then the labels made from the port's
citations in `labels/names-from-port.tsv` (cite_labels.py), then the library code in
`labels/library.tsv` (library.py). `category()` sorts every function into the game's own code,
library code, exception handlers and thunks, for the coverage figures.

Only derived facts are printed by the tools; decompiled text stays local.
"""
import collections
import os
import re
import struct

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
NAMES = os.path.join(ROOT, 'labels', 'names.tsv')
# Labels made from the port's `Original: FUN_` citations (cite_labels.py); names.tsv wins.
PORT_NAMES = os.path.join(ROOT, 'labels', 'names-from-port.tsv')
# The C runtime, the Steam stub and unnamed funclets (library.py); read last.
LIBRARY = os.path.join(ROOT, 'labels', 'library.tsv')
IMAGES = {
    'Application-steam-x64.exe': 'game-data/original/Application-steam-x64.exe',
    'Runtime-steam-x64.dll': 'game-data/original/Runtime-steam-x64.dll',
}
HEADER = re.compile(r'^// ==== ([0-9a-f]+) (\S+)')
CALL = re.compile(r'\bFUN_(1[48][0-9a-f]{7})\b')
DAT = re.compile(r'\b(?:DAT|_DAT)_(1[48][0-9a-f]{7})\b')
STRING = re.compile(r'"((?:[^"\\]|\\.){3,80})"')
TYPEINFO = re.compile(
    r'&DAT_([0-9a-f]+),"([A-Za-z_][A-Za-z0-9_]*)","([A-Za-z_][A-Za-z0-9_:]*)",[^,]*,\s*'
    r'(?:\(\w[^)]*\)\s*&?)?(?:FUN_|LAB_)?([0-9a-f]{9})?', re.S)
VTABLE_ASSIGN = re.compile(r'= &PTR_(?:FUN|LAB)_([0-9a-f]+);')
# The vtable stored at the object's start (`*param_1 = ...`). A class with more than one base
# also stores a second vtable further in (`puVar1 = param_1 + 5; *puVar1 = ...`); its component
# slots are the first one's.
PRIMARY_VTABLE = re.compile(r'\*(?:\([^)]*\))?param_1 = &PTR_(?:FUN|LAB)_([0-9a-f]+);')
ALLOC = re.compile(r'MemAlloc\(0x([0-9a-f]+),')
MENTION = re.compile(r'(?<![0-9a-fA-F])(?:0x|FUN_|DAT_)?(1[48][0-9a-f]{7})(?![0-9a-fA-F])')

# Runtime component vtable slots (docs/ENTITY-COMPONENT-LIFECYCLE.md, docs/CITIZEN-AI.md).
COMPONENT_SLOTS = {
    0x00: 'destructor', 0x08: 'type info', 0x10: 'type id', 0x18: 'serialize',
    0x20: 'setup (bit 0x80)', 0x28: 'pre-destroy (bit 0x100)', 0x30: 'activate (bit 0x01)',
    0x38: 'remove (bit 0x02)', 0x40: 'update with manager delta (bit 0x04)',
    0x48: 'scheduled update with elapsed time (bit 0x08)', 0x50: 'post-iteration (bit 0x10)',
    0x78: 'message',
}
# UI::DialogController slots, from the labelled controllers in names.tsv (Bind x7, Refresh x7,
# HandleEvent x4, Close x1, Serialize x1 at these offsets; docs/BUILDING-WINDOWS.md).
DIALOG_SLOTS = {
    0x20: 'bind (dialog opened)', 0x28: 'serialize (1 labelled)', 0x30: 'close', 0x38: 'refresh',
    0x48: 'handle event',
}
ACTION_SLOTS = {
    0x18: 'serialize', 0x20: 'start', 0x28: 'update(dt)', 0x30: 'notify(dt)', 0x38: 'stop',
    0x40: 'abort',
}


class Image:
    """Typed reads at virtual addresses of a PE32+ image."""

    def __init__(self, path):
        self.data = open(os.path.join(ROOT, path), 'rb').read()
        d = self.data
        pe = struct.unpack_from('<I', d, 0x3C)[0]
        nsec = struct.unpack_from('<H', d, pe + 6)[0]
        opt = struct.unpack_from('<H', d, pe + 20)[0]
        self.base = struct.unpack_from('<Q', d, pe + 24 + 24)[0]
        self.sections = []
        for i in range(nsec):
            s = pe + 24 + opt + 40 * i
            vsize, va, rsize, raw = struct.unpack_from('<IIII', d, s + 8)
            flags = struct.unpack_from('<I', d, s + 36)[0]
            self.sections.append((va, max(vsize, rsize), raw, rsize, bool(flags & 0x20000000)))

    def offset(self, va):
        rva = va - self.base
        for sva, size, raw, rsize, _ in self.sections:
            if sva <= rva < sva + size:
                return raw + rva - sva if rva - sva < rsize else None
        return None

    def is_code(self, va):
        rva = va - self.base
        return any(sva <= rva < sva + size and code
                   for sva, size, _, _, code in self.sections)

    def read(self, va, fmt):
        off = self.offset(va)
        if off is None or off + struct.calcsize(fmt) > len(self.data):
            return None
        return struct.unpack_from(fmt, self.data, off)[0]


FIELDS = os.path.join(ROOT, 'labels', 'fields.tsv')


def load_fields():
    """`Class -> {offset: (name, pointee class or None)}` from fields.tsv."""
    fields = collections.defaultdict(dict)
    if os.path.exists(FIELDS):
        for line in open(FIELDS):
            if line.startswith('#') or not line.strip():
                continue
            cols = line.rstrip('\n').split('\t')
            if len(cols) >= 3:
                fields[cols[0]][int(cols[1], 16)] = (cols[2], cols[3] if len(cols) > 3 and cols[3] else None)
    return fields


def is_handler_name(name):
    """Exception funclets Ghidra named: `Unwind@`, `Catch@`, and `$filt$`/`$fin$` blocks."""
    return name.startswith(('Unwind@', 'Catch@', 'Catch_All@')) or '$filt$' in name \
        or '$fin$' in name or 'filt$0' in name or 'fin$0' in name


def is_thunk_name(name):
    """Import thunks (`RUNTIME-STEAM-X64.DLL::...`) and Ghidra's jump thunks."""
    return '.DLL::' in name or name.startswith('thunk_')


def load_library():
    """`address -> (label, kind, evidence)` from library.tsv."""
    rows = {}
    if os.path.exists(LIBRARY):
        for line in open(LIBRARY):
            if line.startswith('#') or not line.strip():
                continue
            cols = line.rstrip('\n').split('\t')
            if len(cols) >= 4:
                rows[cols[0]] = (cols[1], cols[2], cols[3])
    return rows


def load_labelled():
    """Addresses with a label in names.tsv or names-from-port.tsv: the traced functions."""
    found = set()
    for path in (NAMES, PORT_NAMES):
        if not os.path.exists(path):
            continue
        for line in open(path):
            if line.startswith('#') or not line.strip():
                continue
            found.add(line.split('\t', 1)[0].lower().removeprefix('0x'))
    return found


def load_names():
    names = {}
    for path in (NAMES, PORT_NAMES, LIBRARY):
        if not os.path.exists(path):
            continue
        for line in open(path):
            if line.startswith('#') or not line.strip():
                continue
            cols = line.rstrip('\n').split('\t')
            if len(cols) >= 2:
                names.setdefault(cols[0].lower().removeprefix('0x'), cols[1])
    return names


class Index:
    def __init__(self, binary='Application-steam-x64.exe'):
        self.binary = binary
        base = os.path.join(ROOT, 'analysis', 'export', binary)
        self.size, self.name = {}, {}
        with open(os.path.join(base, 'functions.tsv')) as f:
            for line in f:
                cols = line.rstrip('\n').split('\t')
                if len(cols) >= 3:
                    self.size[cols[0]] = int(cols[1] or 0)
                    self.name[cols[0]] = cols[2]
        self.body = {}
        self.callees = collections.defaultdict(set)
        self.callers = collections.defaultdict(set)
        self.strings = collections.defaultdict(set)
        self.dats = collections.defaultdict(set)
        self.derived = set()
        current, lines = None, []
        text = open(os.path.join(base, 'decomp.c'), errors='replace').read()
        for line in text.split('\n'):
            m = HEADER.match(line)
            if m:
                if current:
                    self.body[current] = '\n'.join(lines)
                current, lines = m.group(1), [line]
                if m.group(2) and not m.group(2).startswith('FUN_'):
                    self.name.setdefault(current, m.group(2))
                continue
            if current is None:
                continue
            lines.append(line)
            for c in CALL.findall(line):
                if c != current:
                    self.callees[current].add(c)
                    self.callers[c].add(current)
            for d in DAT.findall(line):
                self.dats[current].add(d)
            for s in STRING.findall(line):
                self.strings[current].add(s)
            if 'IsDerivedFrom' in line:
                self.derived.add(current)
        if current:
            self.body[current] = '\n'.join(lines)
        # Registered classes: type-info global -> (name, parent, factory).
        self.classes = {}
        for m in TYPEINFO.finditer(text):
            self.classes[m.group(1)] = (m.group(2), m.group(3), m.group(4))
        self.types = {d: v[0] for d, v in self.classes.items()}
        self.traced = self._mentions()
        self.names = load_names()
        self.library = load_library()
        self.labelled = load_labelled()
        self.fields = load_fields()
        self.image = Image(IMAGES[binary]) if binary in IMAGES and os.path.exists(
            os.path.join(ROOT, IMAGES[binary])) else None
        self._vtable_class = None

    def _mentions(self):
        found = collections.defaultdict(set)
        for top in ('docs', 'crates', 'apps', 'tools'):
            for dirpath, dirs, files in os.walk(os.path.join(ROOT, top)):
                dirs[:] = [d for d in dirs if d not in ('target', '__pycache__')]
                for fn in files:
                    if fn in ('names.tsv', 'names-from-port.tsv', 'library.py') or not fn.endswith(('.md', '.rs', '.py', '.wgsl', '.tsv')):
                        continue
                    path = os.path.join(dirpath, fn)
                    try:
                        text = open(path, errors='replace').read()
                    except OSError:
                        continue
                    rel = os.path.relpath(path, ROOT)
                    for a in MENTION.findall(text):
                        found[a].add(rel)
        return found

    # ---- naming -------------------------------------------------------------------------

    def category(self, fn):
        """'game', 'library', 'handler' (exception funclets) or 'thunk'."""
        n = self.name.get(fn, '')
        if is_handler_name(n):
            return 'handler'
        if is_thunk_name(n):
            return 'thunk'
        if fn in self.library:
            return self.library[fn][1]
        return 'game'

    def status(self, fn):
        """'traced' (labelled), 'cited' (mentioned, no label), 'untraced', or the category of
        code that is not the game's own ('library', 'handler', 'thunk')."""
        cat = self.category(fn)
        if cat != 'game':
            return cat
        if fn in self.labelled:
            return 'traced'
        return 'cited' if fn in self.traced else 'untraced'

    def type_names(self, fn):
        return sorted({self.types[d] for d in self.dats.get(fn, ()) if d in self.types})

    def accessor(self, fn):
        types = self.type_names(fn)
        if fn in self.derived and len(types) == 1 and self.size.get(fn, 0) <= 200:
            return f'get<{types[0]}>'
        return None

    def label(self, fn):
        """Best short name: analyst label, Runtime export name, accessor, class role."""
        if fn in self.names:
            return self.names[fn]
        n = self.name.get(fn, '')
        if n and not n.startswith('FUN_'):
            return n.split('::', 1)[-1] if n.startswith('RUNTIME') else n
        role = self.method_role(fn)
        if role:
            return role
        return self.accessor(fn)

    def owner_class(self, fn):
        """The class whose method `fn` is (from its vtable role), if unique."""
        role = self.method_role(fn) or ''
        if '::slot+' in role and '(default' not in role:
            return role.split('::', 1)[0]
        lab = self.names.get(fn, '')
        return lab.split('::', 1)[0] if '::' in lab else None

    def describe(self, fn):
        lab = self.label(fn)
        return f'{fn}{" " + lab if lab else ""} ({self.size.get(fn, 0)} bytes, {self.status(fn)})'

    # ---- classes and vtables ------------------------------------------------------------

    def class_info(self, name):
        for dat, (n, parent, factory) in self.classes.items():
            if n == name:
                return dat, parent, factory
        return None

    def constructor(self, factory):
        """Factory body: allocation size, the constructor it calls, and the vtable installed
        (in the constructor, or inline in the factory)."""
        body = self.body.get(factory, '')
        size = ALLOC.search(body)
        ctor = None
        for c in CALL.findall(body):
            if c != factory:
                ctor = c
                break
        ctor_body = self.body.get(ctor, '') if ctor else ''
        vt = PRIMARY_VTABLE.findall(ctor_body) or VTABLE_ASSIGN.findall(ctor_body)
        if not vt:
            vt = PRIMARY_VTABLE.findall(body) or VTABLE_ASSIGN.findall(body)
        return (int(size.group(1), 16) if size else None, ctor, vt[-1] if vt else None)

    def vtable(self, va, limit=48):
        """Slots until a pointer leaves the code section."""
        if self.image is None:
            return []
        slots = []
        for i in range(limit):
            p = self.image.read(va + 8 * i, '<Q')
            if p is None or not self.image.is_code(p):
                break
            slots.append(f'{p:x}')
        return slots

    def vtable_classes(self):
        """vtable address -> class name, for every registered class with a factory."""
        if self._vtable_class is None:
            self._vtable_class = {}
            for dat, (name, parent, factory) in self.classes.items():
                if factory:
                    _, _, vt = self.constructor(factory)
                    if vt:
                        self._vtable_class[vt] = name
        return self._vtable_class

    def method_role(self, fn):
        """`Class::slot+0x48` when `fn` sits in a registered class's vtable."""
        if not hasattr(self, '_roles'):
            uses = collections.defaultdict(list)
            for vt, name in self.vtable_classes().items():
                for i, s in enumerate(self.vtable(int(vt, 16))):
                    uses[s].append((name, 8 * i))
            self._roles = {}
            for s, owners in uses.items():
                names = {n for n, _ in owners}
                off = collections.Counter(o for _, o in owners).most_common(1)[0][0]
                if len(names) == 1:
                    self._roles[s] = f'{owners[0][0]}::slot+{off:#x}'
                else:
                    # A base implementation shared by several classes: name the common parent.
                    parents = collections.Counter(
                        self.class_info(n)[1] for n in names if self.class_info(n))
                    parent = parents.most_common(1)[0][0] if parents else 'shared'
                    self._roles[s] = f'{parent}::slot+{off:#x} (default, {len(names)} classes)'
        return self._roles.get(fn)

    # ---- constants ------------------------------------------------------------------------

    def constant(self, dat):
        """A readable value for a data global: class name, float, or integer."""
        if dat in self.types:
            return f'type {self.types[dat]}'
        if self.image is None:
            return None
        va = int(dat, 16)
        u = self.image.read(va, '<I')
        if u is None:
            return None
        f = self.image.read(va, '<f')
        exp = (u >> 23) & 0xff
        if u == 0:
            return '0'
        if 0x60 <= exp <= 0x9e:
            return f'{f:.9g}f'
        if u < 0x10000:
            return str(u)
        return None


def norm(a):
    return a.lower().removeprefix('0x').removeprefix('fun_')
