"""List a function's float comparisons and which way a NaN goes at each one.

After `comiss`, `ucomiss`, `comisd` or `ucomisd`, an unordered result (a NaN on either side) sets
the carry, zero and parity flags together. So `jb`, `jbe` and `je` are taken for NaN, while `ja`,
`jae` and `jne` are not. Ghidra writes the test as its ordered meaning (`b <= a`), which is false
for NaN. A port written from that C takes NaN the other way whenever the jump is taken for NaN,
and for `jne`, whose `!=` is true for NaN. Such lines are marked `!`. A mark means "check
this branch in the port": the port is right when its test sends NaN to the same side as the
jump, for example `!(a < b)` for "b <= a or NaN", or the complement in an else branch.

Reads the instructions with objdump from game-data/original (local only) and prints addresses,
mnemonics and operands, no code.

Usage:
  python3 tools/triage/nan_jumps.py 0x140118340 [0x14004a740 ...]
  python3 tools/triage/nan_jumps.py --hazards 0x140118340      # only the marked lines
Options: --binary Application-steam-x64.exe (default) or Runtime-steam-x64.dll.
"""
import argparse
import os
import re
import subprocess
import sys

REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
COMPARES = ('comiss', 'ucomiss', 'comisd', 'ucomisd')
# condition -> (ordered meaning of "b ? a", true after an unordered compare)
CONDITIONS = {
    'a': ('>', False), 'nbe': ('>', False),
    'ae': ('>=', False), 'nb': ('>=', False), 'nc': ('>=', False),
    'b': ('<', True), 'c': ('<', True), 'nae': ('<', True),
    'be': ('<=', True), 'na': ('<=', True),
    'e': ('==', True), 'z': ('==', True),
    'ne': ('!=', False), 'nz': ('!=', False),
    'p': ('unordered', True), 'pe': ('unordered', True),
    'np': ('ordered', False), 'po': ('ordered', False),
}
INSN = re.compile(r'^\s*([0-9a-f]+):\s+(\S+)\s*(.*?)\s*$')
FLAG_WRITERS = ('cmp', 'test', 'add', 'sub', 'and', 'or', 'xor', 'inc', 'dec', 'comis', 'ucomis')


def functions(binary):
    path = os.path.join(REPO, 'analysis', 'export', binary, 'functions.tsv')
    sizes = {}
    with open(path) as f:
        for line in f:
            p = line.rstrip('\n').split('\t')
            if len(p) >= 2 and p[1].isdigit():
                sizes[int(p[0], 16)] = int(p[1])
    return sizes


def disassemble(binary, start, size):
    exe = os.path.join(REPO, 'game-data', 'original', binary)
    out = subprocess.run(['objdump', '-d', '--no-show-raw-insn', f'--start-address={start:#x}',
                          f'--stop-address={start + size:#x}', exe],
                         capture_output=True, text=True, check=True).stdout
    insns = []
    for line in out.splitlines():
        m = INSN.match(line)
        if m:
            insns.append((int(m.group(1), 16), m.group(2), m.group(3).split('#')[0].strip()))
    return insns


def condition(mnemonic):
    for prefix in ('j', 'set', 'cmov'):
        if mnemonic.startswith(prefix):
            rest = mnemonic[len(prefix):]
            if rest in CONDITIONS:
                return prefix, rest
    return None


def report(binary, start, size, hazards_only):
    insns = disassemble(binary, start, size)
    rows = []
    for i, (at, mnem, ops) in enumerate(insns):
        if mnem not in COMPARES:
            continue
        # AT&T order: `comiss a, b` compares b with a.
        parts = [p.strip() for p in re.split(r',(?![^(]*\))', ops)]
        a, b = (parts + ['?', '?'])[:2]
        users, checked_parity = [], False
        for at2, mnem2, ops2 in insns[i + 1:i + 8]:
            c = condition(mnem2)
            if c is None:
                if mnem2.startswith(FLAG_WRITERS):
                    break
                continue
            kind, cc = c
            meaning, nan = CONDITIONS[cc]
            if meaning in ('unordered', 'ordered'):
                checked_parity = True
            hazard = meaning not in ('unordered', 'ordered') and not checked_parity and (
                nan or meaning == '!=')
            users.append((at2, mnem2, ops2, meaning, nan, hazard))
        for at2, mnem2, ops2, meaning, nan, hazard in users or [(None, '-', '', '?', None, False)]:
            if hazards_only and not hazard:
                continue
            where = f'{at2:#x} {mnem2} {ops2}'.strip() if at2 else 'no flag user found'
            text = (f'{b} {meaning} {a}' if meaning not in ('unordered', 'ordered', '?')
                    else meaning)
            nan_text = '' if nan is None else ('NaN: yes' if nan else 'NaN: no')
            rows.append(f"{'!' if hazard else ' '} {at:#x} {mnem} {a}, {b}  ->  {where}  [{text}; {nan_text}]")
    return rows


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument('addresses', nargs='+')
    ap.add_argument('--binary', default='Application-steam-x64.exe')
    ap.add_argument('--hazards', action='store_true', help='print only the marked lines')
    args = ap.parse_args()
    sizes = functions(args.binary)
    for text in args.addresses:
        start = int(text, 16)
        size = sizes.get(start)
        if size is None:
            print(f'{start:#x}: not in functions.tsv', file=sys.stderr)
            continue
        rows = report(args.binary, start, size, args.hazards)
        marked = sum(r.startswith('!') for r in rows)
        print(f'FUN_{start:x} ({size} bytes): {len(rows)} lines, {marked} marked')
        for r in rows:
            print('  ' + r)


if __name__ == '__main__':
    main()
