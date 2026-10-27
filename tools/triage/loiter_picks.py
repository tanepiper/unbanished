#!/usr/bin/env python3
"""The town's draws and Loiter's picks from a winegdb trace of the original.

Run winegdb on the Runtime's Random entries (`RandFloat`, `Rand(a,b)`, `RandTol`,
`RandFloatTol`, `Rand(n)`) and on `0x140110343` in `Activity::Loiter` (`FUN_14010fa30`), where
the picked object's map rectangle is in registers (`r13` x, `r14` y, `ecx` w, `edx` h), with
`--dump rsp:8 --dump rcx:4 --json`. This writes:

- OUT.rng: the draws of the Random that draws most (the town's), one `kind<TAB>caller iINDEX`
  line each, as `rngdiff.py` and `rng_split.py` read them;
- OUT.picks: the rectangle of each object Loiter picked from its address-keyed set, one
  `x y w h` line each, for the port's `--sim-continue --loiter-picks`. A rectangle counts when
  the town's last draw before it was the set's `Rand(n)` (it returns to `0x14010ffdd`); the
  happiness path reaches the same place without it.

Usage: loiter_picks.py TRACE.jsonl OUT.rng OUT.picks [--runtime-base 0x6fffe6db0000]

The Runtime's base is where the bottle loaded `Runtime-steam-x64.dll` (CrossOver's `Steam`
bottle: `0x6fffe6db0000`; docs/ITERATION-ORDER.md).
"""
import argparse
import collections
import json

KINDS = {0x1800ca5b0: 'RandFloat', 0x1800ca5e0: 'Rand({},{})', 0x1800ca610: 'RandTol({})',
         0x1800ca800: 'Rand', 0x1800ca870: 'RandFloatTol', 0x1800ca8b0: 'Rand({})'}
LOITER_RECT = 0x140110343
SET_PICK = 0x14010ffdd


def s32(v):
    v &= 0xffffffff
    return v - (1 << 32) if v & 0x80000000 else v


def hits(path):
    """Each breakpoint hit with its registers and dumps, in order."""
    cur = None
    for line in open(path):
        d = json.loads(line)
        if 'hit' in d:
            if cur:
                yield cur
            cur = {'pc': int(d['pc'], 16), 'regs': {k: int(v, 16) for k, v in d['regs'].items()}}
        elif cur is not None and 'hex' in d:
            cur[d['dump'].split(':')[0]] = int.from_bytes(bytes.fromhex(d['hex']), 'little')
    if cur:
        yield cur


def main():
    p = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    p.add_argument('trace')
    p.add_argument('rng')
    p.add_argument('picks')
    p.add_argument('--runtime-base', type=lambda s: int(s, 0), default=0x6fffe6db0000)
    a = p.parse_args()
    shift = a.runtime_base - 0x180000000
    lo, hi = 0x1800ca560 + shift, 0x1800ca900 + shift
    events = []
    for h in hits(a.trace):
        r = h['regs']
        if h['pc'] == LOITER_RECT:
            events.append(('rect', (s32(r['r13']), s32(r['r14']), s32(r['rcx']), s32(r['rdx']))))
            continue
        ret = h.get('rsp')
        # A draw made inside another Random entry (`Rand(n)` calls `Rand()`) is not the game's.
        if ret is None or lo <= ret < hi:
            continue
        kind = KINDS[h['pc'] - shift]
        if kind == 'Rand({})':
            kind = kind.format(r['rdx'] & 0xffffffff)
        elif kind == 'RandTol({})':
            kind = kind.format(s32(r['rdx']))
        elif kind == 'Rand({},{})':
            kind = kind.format(s32(r['rdx']), s32(r['r8']))
        events.append(('draw', (r['rcx'], kind, ret, h.get('rcx'))))
    counts = collections.Counter(e[1][0] for e in events if e[0] == 'draw')
    town = counts.most_common(1)[0][0]
    draws, picks, last = 0, [], None
    with open(a.rng, 'w') as out:
        for what, e in events:
            if what == 'rect':
                if last == SET_PICK:
                    picks.append(e)
                continue
            obj, kind, ret, index = e
            if obj != town:
                continue
            # A trace cut in the middle of a hit has no index for its last draw.
            if index is None:
                print(f'stopped at draw {draws}: no index')
                break
            out.write(f'{kind}\t0x{ret:x} i{index}\n')
            draws += 1
            last = ret
    with open(a.picks, 'w') as out:
        for x, y, w, h in picks:
            out.write(f'{x} {y} {w} {h}\n')
    print(f'{draws} draws of the town Random at {town:#x}, {len(picks)} Loiter picks')


if __name__ == '__main__':
    main()
