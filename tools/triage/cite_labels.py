#!/usr/bin/env python3
"""(From GameFrame: https://github.com/tanepiper/game-frame, tools/triage/cite_labels.py.)

Make function labels from a reimplementation's citations: every item whose comment says
"Original: FUN_xxxx" names that original function.

Usage: cite_labels.py SOURCE_DIR [--existing names.tsv] [--functions functions.tsv] [--any] [--prefix P]
                      [--notes] [--module NAME [--segment SEG]] [--no-lines] [--out FILE] [--check FILE]

GameFrame's rule is that every ported item names the original function it came from, in a
comment such as `/// Original: FUN_1400be770` (or `// Original: 1400be770`). The port then knows
far more names than labels/names.tsv, and the decompiled code that the next session reads still
says FUN_1400be770. This tool reads the citations and writes a labels file: each cited address
gets the name of the item declared after the comment, a function, a method (Type::method), a
struct, an enum or a class, in Rust, C, C++, Python, JavaScript or TypeScript.

- In a git repository only tracked files are read, so decompiled code and game files in ignored
  folders (analysis/, derived/, game/) are never read. Elsewhere hidden and build folders are
  skipped.
- An "Original:" list that ends with a comma goes on in the next comment line.
- When one item cites several functions, the first gets the item's name and the others get the
  name with _2, _3 and so on after it.
- A citation on a struct's field, an enum's variant or a constant names Type::field,
  Enum::Variant or Block::NAME. A function's or a type's name wins over these for the same
  address, because a field or a variant only mentions a function that something else ports.
- A citation that is not followed by an item (a module's comment, a note in a body) is counted and
  left out.
- --notes also reads a Rust module's note (`//!`): its citations name the module (the file, or its
  folder for mod.rs, lib.rs and main.rs), ranked below every item's name.
- --existing names.tsv: addresses already labelled there by hand are left out, so a person's label
  always wins.
- --out FILE writes the labels (address, label, evidence as file:line); without it they are printed.
- --functions FILE (Ghidra's functions.tsv from the export) keeps only addresses where a function
  starts, which leaves out the ends of ranges and addresses inside functions.
- --any counts the first address in a comment directly above an item, not only those after
  "Original:", for ports that cite as `unit.Procedure` (1000:abf8). Use it with --functions.
  Each function or type takes one such address; fields, variants and constants take none. An
  "Original:" citation of the same address wins.
- --prefix P, with --any: an address outside an "Original:" citation counts only when written
  with P before it (--prefix FUN_). Ports that write instruction addresses as 0x1401259c5 and
  functions as FUN_1401259c0 then need no functions.tsv, so the result is the same on every
  machine.
- --module NAME reads citations written as NAME:offset (an overlay or module and an offset in it,
  `EL2MATH3:1cf5`) and gives the address as SEG:offset, Ghidra's numbering of that overlay
  (--segment, 1000 by default). Run it once for each overlay, with --out labels/NAME.tsv:
  decomp_query.py reads labels/<binary name>.tsv for that binary.
  Other addresses are left out.
- --no-lines gives the evidence as the file alone. A committed labels file then changes only when
  a citation or an item's name changes, not with every edit above it.
- --check FILE exits 1 if FILE differs from what the tool would write, for a test that keeps the
  generated labels current.

Point decomp_query.py at the result with --labels, or keep it beside labels/names.tsv. Standard
library only.
"""
import os
import re
import subprocess
import sys

EXTS = ('.rs', '.c', '.cc', '.cpp', '.h', '.hpp', '.py', '.js', '.ts', '.java', '.go', '.cs', '.pas')
CITE = re.compile(r'Original:\s*(.*)')
ADDR = re.compile(r'`?(?:FUN_)?((?:14|18|0x14|0x18)[0-9a-fA-F]{7}|[0-9a-fA-F]{4}[:_][0-9a-fA-F]{4})`?')
COMMENT = re.compile(r'^\s*(///|//!|//|#|\*|/\*\*|/\*)')
ITEM = [
    re.compile(r'^\s*(?:pub(?:\([^)]*\))?\s+)?(?:const\s+|async\s+|unsafe\s+|extern\s+"C"\s+)*fn\s+([A-Za-z_]\w*)'),
    re.compile(r'^\s*(?:pub(?:\([^)]*\))?\s+)?(?:struct|enum|trait|union|type)\s+([A-Za-z_]\w*)'),
    re.compile(r'^\s*(?:async\s+)?def\s+([A-Za-z_]\w*)'),
    re.compile(r'^\s*(?:class|struct)\s+([A-Za-z_]\w*)'),
    re.compile(r'^\s*(?:export\s+)?(?:async\s+)?function\s+([A-Za-z_]\w*)'),
    re.compile(r'^\s*(?:static\s+|inline\s+|virtual\s+)*[A-Za-z_][\w:<>,\s\*&]*?\s+\**([A-Za-z_][\w:]*)\s*\([^;]*$'),
]
IMPL = re.compile(r'^(\s*)impl(?:<[^>]*>)?\s+(?:[\w:<>]+\s+for\s+)?([A-Za-z_]\w*)')
# Rust blocks whose members can carry a citation: a struct's fields, an enum's variants, a
# module's or a trait's items.
BLOCK = re.compile(r'^(\s*)(?:pub(?:\([^)]*\))?\s+)?(struct|enum|mod|trait|union)\s+([A-Za-z_]\w*)[^;]*\{\s*$')
FIELD = re.compile(r'^\s*(?:pub(?:\([^)]*\))?\s+)?([a-z_]\w*)\s*:(?!:)')
VARIANT = re.compile(r'^\s*([A-Z]\w*)\s*(?:[,({=]|$)')
CONST = re.compile(r'^\s*(?:pub(?:\([^)]*\))?\s+)?(?:const|static)\s+(?:mut\s+)?([A-Za-z_]\w*)\s*:')


def items(lines, c_like=True):
    """For each line, the item it declares (or None). Rust methods are named Type::method, struct
    fields Type::field, enum variants Enum::Variant, constants in a block Block::NAME. Each name
    comes with its rank: 0 for a function or a type, 1 for a field, a variant or a constant, which
    only mention a function that something else ports."""
    out, blocks = [], []  # blocks: (indent, name, kind)
    for line in lines:
        indent = len(line) - len(line.lstrip())
        if line.strip() and line.strip() != '}' and not COMMENT.match(line):
            while blocks and indent <= blocks[-1][0]:
                blocks.pop()
        m = IMPL.match(line)
        if m:
            blocks.append((len(m.group(1)), m.group(2), 'impl'))
            out.append(None)
            continue
        name, rank = None, 1
        inside = blocks[-1] if blocks and indent > blocks[-1][0] else None
        if not COMMENT.match(line):
            if inside and inside[2] == 'struct':
                m = FIELD.match(line)
                name = m and m.group(1)
            elif inside and inside[2] == 'enum':
                m = VARIANT.match(line)
                name = m and m.group(1)
            if not name:
                m = CONST.match(line)
                name = m and m.group(1)
            if not name:
                # The last pattern, a C function declaration, also matches Rust and Python
                # statements (`let Some(x) = f(`), so it is kept for the other languages.
                for pat in ITEM if c_like else ITEM[:-1]:
                    m = pat.match(line)
                    if m:
                        name, rank = m.group(1), 0
                        break
        if name and inside and '::' not in name:
            name = f'{inside[1]}::{name}'
        out.append((name, rank) if name else None)
        m = BLOCK.match(line)
        if m and not COMMENT.match(line):
            blocks.append((len(m.group(1)), m.group(3), m.group(2)))
    return out


def source_files(root):
    """The source files under ROOT: the tracked ones in a git repository, else a walk."""
    try:
        out = subprocess.run(['git', '-C', root, 'ls-files', '-z'], capture_output=True, check=True).stdout
        return [os.path.join(root, f) for f in sorted(out.decode('utf-8', 'replace').split('\0')) if f.endswith(EXTS)]
    except (OSError, subprocess.CalledProcessError):
        found = []
        for base, dirs, files in os.walk(root):
            dirs[:] = sorted(d for d in dirs if d not in ('target', 'build', 'node_modules', 'analysis', 'derived')
                             and not d.startswith('.'))
            found += [os.path.join(base, f) for f in sorted(files) if f.endswith(EXTS)]
        return found


def scan(root, lines_too=True, any_address=False, module=None, segment='1000', starts=None,
         notes=False, prefix=None):
    mod = re.compile(r'\b' + re.escape(module) + r'(?:\.\w+)?:([0-9a-fA-F]{4})\b') if module else None
    pre = re.compile(re.escape(prefix) + r'([0-9a-fA-F]{4,})\b') if prefix else None
    labels, skipped, claimed = [], 0, set()
    for path in source_files(root):
        try:
            lines = open(path, encoding='utf-8', errors='replace').read().split('\n')
        except OSError:
            continue
        names = items(lines, not path.endswith(('.rs', '.py')))
        rel = os.path.relpath(path, root)
        base, f = os.path.split(path)
        stem = os.path.splitext(f)[0]
        module_name = os.path.basename(os.path.abspath(base)) if stem in ('mod', 'lib', 'main') else stem
        where = lambda i: f'{rel}:{i + 1}' if lines_too else rel
        for i, line in enumerate(lines):
            if not COMMENT.match(line):
                continue
            m = CITE.search(line)
            if not m and not (any_address or mod):
                continue
            text = m.group(1) if m else line
            # An "Original:" list that ends with a comma goes on in the next comment line.
            k = i
            while m and text.rstrip().endswith(',') and k + 1 < len(lines) and COMMENT.match(lines[k + 1]):
                k += 1
                text += ' ' + lines[k]
            if mod:
                addrs = [f'{segment}:{a.lower()}' for a in mod.findall(text)]
            elif pre and not m:
                addrs = [a.lower() for a in pre.findall(text)]
            else:
                addrs = [a.lower().removeprefix('0x').replace('_', ':') for a in ADDR.findall(text)]
            if starts is not None:
                addrs = [a for a in addrs if a in starts]
            if any_address and not m:
                addrs = addrs[:1]
            if not addrs:
                continue
            if line.lstrip().startswith('//!'):
                if notes and m:
                    # A module's note: the module's name, ranked 2, below every item's name.
                    for k, addr in enumerate(addrs):
                        labels.append((addr, 2, module_name if k == 0 else f'{module_name}_{k + 1}', where(i)))
                else:
                    skipped += len(addrs)
                continue
            name = None
            for j in range(i + 1, min(i + 40, len(lines))):
                if names[j]:
                    name = names[j]
                    break
                if lines[j].strip() and not COMMENT.match(lines[j]) and not lines[j].lstrip().startswith(('#[', '@')):
                    break
            if not name:
                skipped += len(addrs)
                continue
            name, rank = name
            if any_address and not m:
                # A field, a variant or a constant only mentions a function ("zero in
                # FUN_x's constructor"); without "Original:" only a function or a type ports it.
                if rank or (path, j) in claimed:
                    continue
                claimed.add((path, j))
                # Below every "Original:" citation of the same address.
                rank += 3
            for k, addr in enumerate(addrs):
                labels.append((addr, rank, name if k == 0 else f'{name}_{k + 1}', where(i)))
    return labels, skipped


def load(path):
    out = set()
    if path and os.path.exists(path):
        for line in open(path, encoding='utf-8'):
            if line.strip() and not line.startswith('#'):
                out.add(line.split('\t')[0].strip().lower().removeprefix('0x').removeprefix('fun_'))
    return out


def render(labels, existing):
    seen, rows = set(), []
    # Per address: a function's or a type's name first (rank 0), then the first name in order.
    for addr, _, name, where in sorted(labels):
        if addr in existing or addr in seen:
            continue
        seen.add(addr)
        rows.append(f'{addr}\t{name}\t{where}')
    head = ('# Labels made from the port\'s citations by tools/triage/cite_labels.py: address, the item that cites\n'
            '# it, and where it is cited. Do not edit: label by hand in names.tsv, which wins over this file.\n')
    return head + '\n'.join(rows) + '\n', len(rows)


def main(argv):
    if not argv or argv[0] in ('-h', '--help'):
        print(__doc__)
        return 0
    opt = lambda name: argv[argv.index(name) + 1] if name in argv else None
    starts = None
    if opt('--functions'):
        starts = set()
        for row in open(opt('--functions'), encoding='utf-8', errors='replace'):
            a = row.split('\t')[0].strip().lower().removeprefix('0x').replace('_', ':')
            if a and not a.startswith('#'):
                starts.add(a)
    labels, skipped = scan(argv[0], '--no-lines' not in argv, '--any' in argv, opt('--module'), opt('--segment') or '1000',
                           starts, '--notes' in argv, opt('--prefix'))
    text, n = render(labels, load(opt('--existing')))
    if opt('--check'):
        old = open(opt('--check'), encoding='utf-8').read() if os.path.exists(opt('--check')) else ''
        if old != text:
            print(f'{opt("--check")} is out of date: run cite_labels.py again')
            return 1
        return 0
    if opt('--out'):
        with open(opt('--out'), 'w', encoding='utf-8') as f:
            f.write(text)
    else:
        sys.stdout.write(text)
    print(f'{n} labels from {len(labels)} citations; {skipped} citations with no item after them', file=sys.stderr)
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
