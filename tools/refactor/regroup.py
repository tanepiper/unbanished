"""Move a crate's top-level modules into group folders and rewrite every path to them.

Usage: python3 tools/refactor/regroup.py CRATE_DIR ROOT CRATE_NAME PLAN.json [--dry-run]

ROOT is the crate root file (`lib.rs` or `main.rs`); CRATE_NAME is how other crates name it
(`unbanished_game`), or `-` for a binary. PLAN.json maps a group to its doc line, modules and
any other files that move with them (shader sources a module includes):
  {"people": {"doc": "Citizens: ...", "modules": ["citizens", "life"], "files": []}, ...}

For each module `m` moved into group `g` it:
  - `git mv`s CRATE_DIR/src/m.rs (or the folder m/) to CRATE_DIR/src/g/;
  - writes CRATE_DIR/src/g/mod.rs with the doc line and `pub mod` lines;
  - replaces the module's line in ROOT (`mod m;` or `pub mod m;`, with any attributes above
    it, which move to the group's mod.rs) with `mod g;` (once per group);
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
    crate_dir, root, crate, plan_path = Path(sys.argv[1]), sys.argv[2], sys.argv[3], sys.argv[4]
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
        if crate != "-":
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

    lib = src / root
    text = lib.read_text()
    public = "pub mod" in text
    for g, v in plan.items():
        (src / g).mkdir(exist_ok=True)
        for m in v["modules"]:
            old = src / f"{m}.rs" if (src / f"{m}.rs").exists() else src / m
            subprocess.run(["git", "mv", str(old), str(src / g / old.name)], check=True)
        for f in v.get("files", []):
            subprocess.run(["git", "mv", str(src / f), str(src / g / f)], check=True)
        lines = []
        first = True
        for m in sorted(v["modules"]):
            decl = re.search(rf"((?:[ \t]*#\[[^\n]*\]\n)*)(?:pub(?:\(crate\))? )?mod {m};\n", text)
            attrs = decl.group(1) if decl else ""
            lines.append(f"{attrs}pub mod {m};\n")
            if decl:
                replacement = f"{'pub ' if public else ''}mod {g};\n" if first else ""
                text = text[: decl.start()] + replacement + text[decl.end():]
                first = False
        (src / g / "mod.rs").write_text(f"//! {v['doc']}\n\n{''.join(lines)}")
    lib.write_text(text)
    print("moved", sum(len(v["modules"]) for v in plan.values()), "modules into", len(plan), "groups")


if __name__ == "__main__":
    main()
