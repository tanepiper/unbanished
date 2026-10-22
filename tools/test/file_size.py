#!/usr/bin/env python3
"""Keep source files small: no Rust file may pass LIMIT lines.

Files that were already larger when the limit came in are listed in tools/test/large-files.txt
with their length then. A listed file may shrink, never grow; when it is split below the limit,
its line is removed. A new file over the limit, or a listed file that grew, fails the check.

Usage:
  file_size.py            check (exit 1 with the files at fault)
  file_size.py --update   rewrite the list from the files over the limit now (only after a split)
"""
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
LIST = REPO / "tools/test/large-files.txt"
LIMIT = 500


def lengths() -> dict[str, int]:
    files = subprocess.run(["git", "ls-files", "--cached", "--others", "--exclude-standard", "*.rs"],
                           cwd=REPO, capture_output=True, text=True, check=True).stdout.split()
    return {f: len((REPO / f).read_text(errors="replace").splitlines()) for f in files if (REPO / f).exists()}


def listed() -> dict[str, int]:
    out = {}
    for line in LIST.read_text().splitlines() if LIST.exists() else []:
        if line.strip() and not line.startswith("#"):
            n, f = line.split(maxsplit=1)
            out[f] = int(n)
    return out


def main() -> int:
    now = lengths()
    if "--update" in sys.argv:
        old = listed()
        rows = sorted(((n, f) for f, n in now.items() if n > LIMIT), reverse=True)
        grew = [f for n, f in rows if f in old and n > old[f]]
        if grew:
            print("not updated: these listed files grew; split them instead:\n  " + "\n  ".join(grew))
            return 1
        LIST.write_text(f"# Rust files over {LIMIT} lines when tools/test/file_size.py came in, with their length\n"
                        "# then. Each may shrink, never grow. Split them, and remove the line.\n"
                        + "".join(f"{n} {f}\n" for n, f in rows))
        print(f"{len(rows)} files over {LIMIT} lines listed")
        return 0
    allowed = listed()
    faults = [f"{f}: {n} lines (the limit is {LIMIT})" for f, n in sorted(now.items())
              if n > LIMIT and f not in allowed]
    faults += [f"{f}: {n} lines, listed at {allowed[f]}: it may not grow" for f, n in sorted(now.items())
               if f in allowed and n > allowed[f]]
    if faults:
        print("\n".join(faults))
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
