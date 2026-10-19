"""Move a source file's inline test modules into their own files.

Usage: python3 tools/refactor/split_tests.py FILE.rs [FILE.rs ...] [--min-lines N] [--dry-run]

Every top-level `#[cfg(...test...)] mod NAME { ... }` block of at least N lines (default 1) in
`dir/file.rs` moves to `dir/file/NAME.rs` (or `dir/NAME.rs` for a `mod.rs`/`lib.rs`/`main.rs`),
dedented by one level, and the block becomes `#[cfg(test)] mod NAME;`. `use super::*` inside
keeps working: the test module's parent is still the same file's module. Braces are matched
with strings, chars and comments skipped.
"""
import re
import sys
from pathlib import Path


def block_end(s, i):
    """Index just past the brace that closes the one at s[i]."""
    depth = 0
    n = len(s)
    while i < n:
        c = s[i]
        if s.startswith("//", i):
            i = s.find("\n", i)
            i = n if i < 0 else i
            continue
        if s.startswith("/*", i):
            i = s.index("*/", i) + 2
            continue
        m = re.match(r'(b?r)(#*)"', s[i:i + 40])
        if m and (i == 0 or not (s[i - 1].isalnum() or s[i - 1] == "_")):
            close = '"' + m.group(2)
            i = s.index(close, i + len(m.group(0))) + len(close)
            continue
        if c == '"':
            i += 1
            while s[i] != '"':
                i += 2 if s[i] == "\\" else 1
            i += 1
            continue
        if c == "'":
            # A char literal ('x', '\n', '\u{..}'), not a lifetime ('a).
            m = re.match(r"'(\\.|\\u\{[0-9a-fA-F]+\}|[^\\'])'", s[i:i + 12])
            if m:
                i += len(m.group(0))
                continue
        if c == "{":
            depth += 1
        elif c == "}":
            depth -= 1
            if depth == 0:
                return i + 1
        i += 1
    raise ValueError("unbalanced braces")


def split(path, min_lines, dry):
    p = Path(path)
    s = p.read_text()
    pattern = re.compile(
        r"^(#\[cfg\([^\n]*\btest\b[^\n]*\)\]\n)((?:#\[[^\n]*\]\n)*)mod ([a-z_][a-z0-9_]*) \{", re.M
    )
    out_dir = p.parent if p.name in ("mod.rs", "lib.rs", "main.rs") else p.parent / p.stem
    moved = []
    pos = 0
    while True:
        m = pattern.search(s, pos)
        if not m:
            break
        open_brace = m.end() - 1
        end = block_end(s, open_brace)
        body = s[open_brace + 1:end - 1]
        lines = body.strip("\n").splitlines()
        if len(lines) < min_lines:
            pos = end
            continue
        dedented = "\n".join(l[4:] if l.startswith("    ") else l for l in lines) + "\n"
        name = m.group(3)
        target = out_dir / f"{name}.rs"
        if target.exists():
            raise SystemExit(f"{target} exists")
        if not dry:
            out_dir.mkdir(parents=True, exist_ok=True)
            target.write_text(dedented)
        replacement = f"{m.group(1)}{m.group(2)}mod {name};"
        s = s[:m.start()] + replacement + s[end:]
        pos = m.start() + len(replacement)
        moved.append((name, len(lines), target))
    if moved and not dry:
        p.write_text(s)
    for name, n, target in moved:
        print(f"{path}: mod {name} ({n} lines) -> {target}")


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    min_lines = 1
    if "--min-lines" in sys.argv:
        min_lines = int(sys.argv[sys.argv.index("--min-lines") + 1])
        args.remove(str(min_lines))
    for f in args:
        split(f, min_lines, "--dry-run" in sys.argv)


if __name__ == "__main__":
    main()
