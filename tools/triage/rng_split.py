"""Split the original's draw trace into the draws to compare and the draws to replay.

The trace (one line per draw: `kind<TAB>caller iINDEX`, the generator's index before the draw)
leaves plain `Rand()` out, so each gap in the index is that many plain draws. Plain draws and
the draws of the callers named with --skip (the music player, `0x1400a98d2`) are the original's
sounds. This writes the other draws, for `rngdiff.py`, and per draw the number of sound draws
made before it, for `--sim-continue --replay-draws`, which makes those draws in the port.

  python3 tools/triage/rng_split.py ORIGINAL.rng COMPARE.rng SKIPS.txt [--skip 0x1400a98d2]
"""
import argparse


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    p.add_argument('trace')
    p.add_argument('compare')
    p.add_argument('skips')
    p.add_argument('--skip', action='append', default=['0x1400a98d2'])
    args = p.parse_args()
    compare, skips = open(args.compare, 'w'), open(args.skips, 'w')
    pending, prev = 0, None
    for line in open(args.trace):
        kind, rest = line.rstrip('\n').split('\t')
        caller, index = rest.split(' ')
        i = int(index[1:])
        if prev is not None:
            pending += (i - prev - 1) % 624
        prev = i
        if caller in args.skip:
            pending += 1
            continue
        compare.write(f'{kind}\t{caller}\n')
        skips.write(f'{pending}\n')
        pending = 0


if __name__ == '__main__':
    main()
