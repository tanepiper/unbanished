"""Every function of the Application that draws from the game's Random, with what the port has.

Usage: python3 tools/triage/draw_sites.py [--missing]

Reads analysis/export/Application-steam-x64.exe/decomp.c and lists each function that calls a
System::Random method on the game's generator (Game +0xd8, through DAT_1401cbdd8), with the
number of such calls, its label, whether the port cites it (crates/, apps/) and whether an
emulator script or test names it (tools/emu/, crates/*/tests/). docs/RANDOM-DRAWS.md records
what each one was found to do. --missing lists only the functions with no emulator check.
"""
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
DECOMP = os.path.join(ROOT, 'analysis/export/Application-steam-x64.exe/decomp.c')
DRAW = re.compile(r'System::Random::Rand\w*\(\*\(Random \*\*\)\(DAT_1401cbdd8 \+ 0xd8\)'
                  r'|System::Random::Rand\w*\(this\b|System::Random::Rand\w*\(pRVar\d+')


def labels():
    names = {}
    for f in ('labels/names.tsv', 'labels/names-from-port.tsv'):
        try:
            for line in open(os.path.join(ROOT, f)):
                p = line.rstrip('\n').split('\t')
                if len(p) >= 2:
                    names.setdefault(p[0].lower().removeprefix('0x').lstrip('0'), p[1])
        except FileNotFoundError:
            pass
    return names


def mentions(addr, paths):
    r = subprocess.run(['grep', '-rl', addr] + paths, cwd=ROOT, capture_output=True, text=True)
    return r.stdout.split()


def main():
    if not os.path.exists(DECOMP):
        sys.exit(f'{DECOMP} is missing: run tools/ghidra/export.sh first')
    counts, current, game_rng = {}, None, False
    for line in open(DECOMP, errors='replace'):
        if line.startswith('// ==== '):
            current, game_rng = line.split()[2], False
            continue
        if 'DAT_1401cbdd8 + 0xd8' in line:
            game_rng = True
        if current and (DRAW.search(line) and (game_rng or 'DAT_1401cbdd8 + 0xd8' in line)):
            counts[current] = counts.get(current, 0) + 1
    names = labels()
    tests = [p for p in ('tools/emu', 'crates/game/tests', 'crates/sim/tests', 'crates/core/tests')
             if os.path.isdir(os.path.join(ROOT, p))]
    for addr in sorted(counts):
        ported = bool(mentions(addr, ['crates', 'apps']))
        measured = bool(mentions(addr, tests))
        if '--missing' in sys.argv and measured:
            continue
        status = 'measured' if measured else ('ported' if ported else 'not ported')
        print(f'{addr}\t{counts[addr]}\t{status}\t{names.get(addr.lstrip("0"), "")}')


if __name__ == '__main__':
    main()
