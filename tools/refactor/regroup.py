"""Move a crate's top-level modules into group folders and rewrite every path to them.

Usage: python3 tools/refactor/regroup.py CRATE_DIR CRATE_NAME PLAN.json [--dry-run]

PLAN.json maps a group to its doc line and modules:
  {"people": {"doc": "Citizens: ...", "modules": ["citizens", "life"]}, ...}

For each module `m` moved into group `g` it:
  - `git mv`s CRATE_DIR/src/m.rs (or the folder m/) to CRATE_DIR/src/g/;
  - writes CRATE_DIR/src/g/mod.rs with the doc line and `pub mod` lines;
  - replaces `pub mod m;` in lib.rs with `pub mod g;` (once per group);
  - rewrites `crate::m` to `crate::g::m` inside the crate, and `CRATE_NAME::m` to
    `CRATE_NAME::g::m` in every tracked .rs file of the workspace.
Grouped imports (`use CRATE_NAME::{a, b}`) are listed for fixing by hand. The compiler is the
check: build and test after running it.
"""
import json
import re
import subprocess
import sys
from pathlib import Path


def main():
    crate_dir, crate, plan_path = Path(sys.argv[1]), sys.argv[2], sys.argv[3]
    dry = "--dry-run" in sys.argv
    plan = json.loads(Path(plan_path).read_text())
    src = crate_dir / "src"
    moves = {m: g for g, v in plan.items() for m in v["modules"]}

    files = subprocess.run(["git", "ls-files", "*.rs"], capture_output=True, text=True,
                           check=True).stdout.split()
    names = "|".join(sorted(moves, key=len, reverse=True))
    inner = re.compile(rf"\bcrate::({names})\b")
    outer = re.compile(rf"\b{crate}::({names})\b")
    grouped = re.compile(rf"\b{crate}::\{{")
    changed = 0
    for f in files:
        p = Path(f)
        s = p.read_text()
        o = s
        if p.is_relative_to(crate_dir):
            s = inner.sub(lambda m: f"crate::{moves[m.group(1)]}::{m.group(1)}", s)
        s = outer.sub(lambda m: f"{crate}::{moves[m.group(1)]}::{m.group(1)}", s)
        for line_no, line in enumerate(s.splitlines(), 1):
            if grouped.search(line):
                print(f"check grouped import: {f}:{line_no}: {line.strip()}")
        if s != o:
            changed += 1
            if not dry:
                p.write_text(s)
    print(f"rewrote paths in {changed} files")
    if dry:
        return

    lib = src / "lib.rs"
    text = lib.read_text()
    for g, v in plan.items():
        (src / g).mkdir(exist_ok=True)
        for m in v["modules"]:
            old = src / f"{m}.rs" if (src / f"{m}.rs").exists() else src / m
            new = src / g / old.name
            subprocess.run(["git", "mv", str(old), str(new)], check=True)
        body = "".join(f"pub mod {m};\n" for m in sorted(v["modules"]))
        (src / g / "mod.rs").write_text(f"//! {v['doc']}\n\n{body}")
        first = True
        for m in v["modules"]:
            line = f"pub mod {m};\n"
            if first:
                text = text.replace(line, f"pub mod {g};\n", 1)
                first = False
            else:
                text = text.replace(line, "", 1)
    lib.write_text(text)
    print("moved", sum(len(v["modules"]) for v in plan.values()), "modules into", len(plan), "groups")


if __name__ == "__main__":
    main()
