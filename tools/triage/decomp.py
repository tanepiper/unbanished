"""Readable decompilation of one original function, annotated with everything the project
knows about the names in it.

  python3 tools/triage/decomp.py 0x1401148b0            # annotated, boilerplate collapsed
  python3 tools/triage/decomp.py 0x1401148b0 --raw      # Ghidra text as exported
  python3 tools/triage/decomp.py 0x1401148b0 --asm      # objdump of the function's bytes
  python3 tools/triage/decomp.py 0x14003b73c --headless # decompile with headless Ghidra when
                                                        # the bulk export lacks the function
  --binary Runtime-steam-x64.dll for the engine DLL.

Annotations: calls get their label (`/* CitizenAIComponent::slot+0x48 */`, `get<Type>`,
analyst names from names.tsv, Runtime export names) and traced status; data globals get their
class name or constant value (`DAT_1401787d4 /* 0.05f */`); vtable stores name the class.
Collapsed: `InternalBasePtr::CheckPtr` noise and the inlined `Collection::Array` grow-and-append
blocks (`/* array append */`).

Output is for reading locally; do not paste decompiled code into committed files.
"""
import argparse
import os
import re
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from ghidra_index import CALL, ROOT, IMAGES, Index, norm  # noqa: E402

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from re_env import analyze_headless, java_home  # noqa: E402  (.env, then the usual places)

GHIDRA = analyze_headless()
JAVA_HOME = java_home()
PROJECTS = {
    'Application-steam-x64.exe': ('analysis/gameplay-project', 'banished-gameplay'),
    'Runtime-steam-x64.dll': ('analysis/runtime-identity-project', 'banished-runtime'),
}
CACHE = os.path.join(ROOT, 'analysis', 'triage-cache')


CHECK_STMT = re.compile(r'^\s*Object::InternalBasePtr::CheckPtr\((?:[^()]|\([^()]*\))*\);\s*$')
CHECK_EXPR = re.compile(r'Object::InternalBasePtr::CheckPtr\((?:[^()]|\([^()]*\))*\),\s*')


def strip_checks(lines):
    """Drop `InternalBasePtr::CheckPtr` statements and comma-expression uses (null checks the
    original inserts before every smart-pointer dereference)."""
    return [CHECK_EXPR.sub('', ln) for ln in lines if not CHECK_STMT.match(ln)]


def collapse_arrays(lines):
    """Replace inlined array growth (MemAlloc + element copy loop + MemFree) with one line."""
    out, i = [], 0
    while i < len(lines):
        line = lines[i]
        if re.match(r'\s*if \(\(?\w+\)? [!=]= \(?\w+\)?\) \{$', line):
            depth, j = 0, i
            while j < len(lines):
                depth += lines[j].count('{') - lines[j].count('}')
                if depth <= 0:
                    break
                j += 1
            block = '\n'.join(lines[i:j + 1])
            if (j - i > 8 and 'MemAlloc' in block and 'do {' in block
                    and not CALL.search(block)
                    and re.search(r'\[1\] = \w+\[1\]|\+ 4\) = \*\(\w+ \*\)', block)):
                indent = re.match(r'\s*', line).group(0)
                out.append(f'{indent}/* array append */')
                i = j + 1
                continue
        m = re.match(r'(\s*)(\w+) = \(void \*\)\*\w+;$', line)
        if m:
            var, k = m.group(2), i
            while k < len(lines) and k - i < 80 and f'MemFree({var})' not in lines[k]:
                k += 1
            if k < len(lines) and k - i < 80:
                balance = sum(l.count('{') - l.count('}') for l in lines[i:k + 1])
                while balance > 0 and k + 1 < len(lines):
                    k += 1
                    balance += lines[k].count('{') - lines[k].count('}')
                block = '\n'.join(lines[i:k + 1])
                if balance == 0 and 'MemAlloc' in block and 'do {' in block:
                    out.append(f'{m.group(1)}/* array append */')
                    i = k + 1
                    continue
        out.append(line)
        i += 1
    return out


def annotate(ix, text, own=None):
    vt = ix.vtable_classes()

    def fun(m):
        a = m.group(1)
        if a == own:
            return m.group(0)
        lab = ix.label(a)
        mark = '' if a in ix.traced else ' untraced'
        return f'FUN_{a}' + (f' /* {lab}{mark} */' if lab else (' /* untraced */' if mark else ''))

    def dat(m):
        a = m.group(2)
        c = ix.constant(a)
        return m.group(0) + (f' /* {c} */' if c else '')

    def ptr(m):
        a = m.group(1)
        return m.group(0) + (f' /* vtable {vt[a]} */' if a in vt else '')

    text = re.sub(r'\bFUN_(1[48][0-9a-f]{7})\b(?!\s*/\*)', fun, text)
    text = re.sub(r'\b(_?DAT)_(1[48][0-9a-f]{7})\b', dat, text)
    text = re.sub(r'&PTR_(?:FUN|LAB)_([0-9a-f]+)', ptr, text)
    return text


def annotate_fields(ix, text, cls):
    """Name `param_1 + 0xNN` after the owner class's fields, and one level of typed pointers
    (`*(longlong *)(param_1 + 0x38) + 0x44` with field 0x38 pointing at another class)."""
    fields = ix.fields.get(cls)
    if not fields:
        return text

    def chase(m):
        outer, inner = int(m.group(1), 16), int(m.group(2), 16)
        f = fields.get(outer)
        if not f or not f[1]:
            return m.group(0)
        sub = ix.fields.get(f[1], {}).get(inner)
        return m.group(0) + f' /* {f[0]}.{sub[0] if sub else hex(inner)} */'

    def direct(m):
        off = int(m.group(1), 16)
        f = fields.get(off)
        return m.group(0) + (f' /* .{f[0]} */' if f else '')

    text = re.sub(r'\*\(longlong \*\)\(param_1 \+ 0x([0-9a-f]+)\)\) \+ 0x([0-9a-f]+)(?![0-9a-f])', chase, text)
    text = re.sub(r'\*\(longlong \*\)\(param_1 \+ 0x([0-9a-f]+)\) \+ 0x([0-9a-f]+)(?![0-9a-f])', chase, text)
    text = re.sub(r'\(param_1 \+ 0x([0-9a-f]+)\)(?! \+)(?!\s*/\*)', direct, text)

    def path_class(path):
        c = cls
        for part in path.split('.'):
            f = next((v for v in ix.fields.get(c, {}).values() if v[0] == part), None)
            if not f or not f[1]:
                return None
            c = f[1]
        return c

    def deeper(m):
        path, off = m.group(1), int(m.group(2), 16)
        c = path_class(path)
        sub = ix.fields.get(c, {}).get(off) if c else None
        if not c:
            return m.group(0)
        return m.group(0) + f' /* {path}.{sub[0] if sub else hex(off)} */'

    for _ in range(3):
        text = re.sub(r'/\* ([\w.\[\]]+) \*/\) \+ 0x([0-9a-f]+)(?![0-9a-f])(?!\s*/\*)', deeper, text)
    return text


def headless(binary, addr):
    os.makedirs(CACHE, exist_ok=True)
    out = os.path.join(CACHE, f'{binary}-{addr}.txt')
    if not os.path.exists(out):
        project, name = PROJECTS[binary]
        env = dict(os.environ, JAVA_HOME=JAVA_HOME)
        # The real path: Ghidra rejects a project path with a dot folder (.claude/worktrees/...).
        subprocess.run([GHIDRA, os.path.realpath(os.path.join(ROOT, project)), name, '-process', binary,
                        '-noanalysis', '-readOnly', '-scriptPath',
                        os.path.join(ROOT, 'tools/ghidra'), '-postScript',
                        'ExportAddressBatch.java', out, f'0x{addr}'],
                       check=True, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    return open(out, errors='replace').read()


def disassemble(ix, binary, addr):
    size = ix.size.get(addr) or 0x200
    start = int(addr, 16)
    return subprocess.run(
        ['objdump', '-d', '--no-show-raw-insn', f'--start-address={start:#x}',
         f'--stop-address={start + size:#x}', os.path.join(ROOT, IMAGES[binary])],
        capture_output=True, text=True).stdout


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    p.add_argument('address')
    p.add_argument('--binary', default='Application-steam-x64.exe')
    p.add_argument('--raw', action='store_true')
    p.add_argument('--asm', action='store_true')
    p.add_argument('--headless', action='store_true')
    args = p.parse_args()
    ix = Index(args.binary)
    addr = norm(args.address)
    print(f'// {ix.describe(addr)}; {len(ix.callers.get(addr, ()))} callers')
    if addr in ix.traced:
        print('// mentioned in:', ', '.join(sorted(ix.traced[addr])[:6]))
    if args.asm:
        print(annotate(ix, disassemble(ix, args.binary, addr)))
        return
    text = ix.body.get(addr)
    if text is None:
        if not args.headless:
            print(f'// {addr} is not in the bulk export; rerun with --headless (about a minute)')
            return 1
        text = headless(args.binary, addr)
    if args.raw:
        print(text)
        return
    lines = strip_checks(text.split('\n'))
    text = annotate(ix, '\n'.join(collapse_arrays(lines)), addr)
    cls = ix.owner_class(addr)
    if cls:
        print(f'// fields of {cls} from tools/triage/fields.tsv')
        text = annotate_fields(ix, text, cls)
    print(text)


if __name__ == '__main__':
    sys.exit(main())
