#!/usr/bin/env python3
"""(From GameFrame: https://github.com/tanepiper/game-frame, tools/triage/cite_labels.py.)

Make function labels from a reimplementation's citations: every item whose comment says
"Original: FUN_xxxx" names that original function.

Usage: cite_labels.py SOURCE_DIR [--existing names.tsv] [--no-lines] [--out FILE] [--check FILE]

GameFrame's rule is that every ported item names the original function it came from, in a
comment such as `/// Original: FUN_1400be770` (or `// Original: 1400be770`). The port then knows
far more names than labels/names.tsv, and the decompiled code that the next session reads still
says FUN_1400be770. This tool reads the citations and writes a labels file: each cited address
gets the name of the item declared after the comment, a function, a method (Type::method), a
struct, an enum or a class, in Rust, C, C++, Python, JavaScript or TypeScript.

- When one item cites several functions, the first gets the item's name and the others get the
  name with _2, _3 and so on after it.
- A citation that is not followed by an item (a module's comment, a note in a body) is counted and
  left out.
- --existing names.tsv: addresses already labelled there by hand are left out, so a person's label
  always wins.
- --out FILE writes the labels (address, label, evidence as file:line); without it they are printed.
- --no-lines gives the evidence as the file alone. A committed labels file then changes only when
  a citation or an item's name changes, not with every edit above it.
- --check FILE exits 1 if FILE differs from what the tool would write, for a test that keeps the
  generated labels current.

Point decomp_query.py at the result with --labels, or keep it beside labels/names.tsv. Standard
library only.
"""
import os
import re
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


def items(lines):
    """For each line, the item it declares (or None), with Rust methods named Type::method."""
    out, impls = [], []
    for line in lines:
        m = IMPL.match(line)
        if m:
            impls.append((len(m.group(1)), m.group(2)))
            out.append(None)
            continue
        indent = len(line) - len(line.lstrip())
        if line.strip() and line.strip() != '}' and not COMMENT.match(line):
            while impls and indent <= impls[-1][0]:
                impls.pop()
        name = None
        if not COMMENT.match(line):
            for pat in ITEM:
                m = pat.match(line)
                if m:
                    name = m.group(1)
                    break
        if name and impls and indent > impls[-1][0] and '::' not in name:
            name = f'{impls[-1][1]}::{name}'
        out.append(name)
    return out


def scan(root, lines_too=True):
    labels, skipped = [], 0
    for base, dirs, files in os.walk(root):
        # Dot folders hold copies (.claude/worktrees) and tool state; GameFrame skips them too.
        dirs[:] = sorted(d for d in dirs if d not in ('target', 'build', 'node_modules')
                         and not d.startswith('.'))
        for f in sorted(files):
            if not f.endswith(EXTS):
                continue
            path = os.path.join(base, f)
            try:
                lines = open(path, encoding='utf-8', errors='replace').read().split('\n')
            except OSError:
                continue
            names = items(lines)
            for i, line in enumerate(lines):
                if not COMMENT.match(line):
                    continue
                m = CITE.search(line)
                if not m:
                    continue
                addrs = [a.lower().removeprefix('0x').replace('_', ':') for a in ADDR.findall(m.group(1))]
                if not addrs:
                    continue
                if line.lstrip().startswith('//!'):
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
                rel = os.path.relpath(path, root)
                for k, addr in enumerate(addrs):
                    labels.append((addr, name if k == 0 else f'{name}_{k + 1}', f'{rel}:{i + 1}' if lines_too else rel))
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
    for addr, name, where in sorted(labels):
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
    labels, skipped = scan(argv[0], '--no-lines' not in argv)
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
