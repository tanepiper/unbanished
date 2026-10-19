"""Move a type's methods and some free items out of a large file into child modules by topic.

Usage: python3 tools/refactor/split_impl.py PLAN.json [--dry-run]

PLAN.json:
  {"file": "apps/game/src/gameplay.rs", "type": "GameplayLoop",
   "modules": {"saving": {"doc": "Saving and loading.", "methods": ["save_state", ...],
                          "items": ["SaveTarget", "achievement_save"],
                          "impls": ["impl GameplayLoop {\\n    /// The window events"]}}}

For each child module `m` it writes `<dir>/<stem>/m.rs` (or `<dir>/m.rs` next to a mod.rs) with
the doc line, `use super::*;`, the listed items (top-level `fn`/`enum`/`struct`/`const`, with
their doc comments and attributes) and one `impl TYPE { ... }` holding the listed methods, and
deletes them from the file, which gains `mod m;` and `use m::{items};`. Moved methods and items
that were private become `pub(super)` so the parent and the other children can still use them.
Moved `pub` and `pub(crate)` items are re-exported with the same visibility, so their paths
stay the same. `impls` moves whole impl blocks, found by their first lines; a trait impl (`impl T for X`)
keeps its methods as they are. Code is moved, never changed: the
compiler and the tests are the check.
"""
import json
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from split_tests import block_end  # noqa: E402


def leading(s, start):
    """Start of the doc comments and attributes directly above `start` (a line start)."""
    lines = s[:start].split("\n")[:-1]
    keep = 0
    for line in reversed(lines):
        t = line.strip()
        if t.startswith("///") or t.startswith("#[") or t.startswith("//"):
            keep += 1
        else:
            break
    cut = sum(len(l) + 1 for l in lines[: len(lines) - keep])
    return cut


def item_end(s, i, kind):
    """End of the item whose header ends at `i`: a `fn`, `struct` or `enum` ends at the brace
    that closes its body (or a `;` for a unit or tuple struct); a `const`, `static` or `type`
    ends at its `;`, skipping any braces in its value. Brackets, strings and chars are skipped."""
    depth = 0
    while i < len(s):
        c = s[i]
        if c == '"':
            i += 1
            while s[i] != '"':
                i += 2 if s[i] == "\\" else 1
        elif c in "([":
            depth += 1
        elif c in ")]":
            depth -= 1
        elif c == "{" and depth == 0:
            end = block_end(s, i)
            if kind in ("fn", "struct", "enum"):
                return end
            i = end
            continue
        elif c == ";" and depth == 0:
            return i + 1
        i += 1
    raise SystemExit("item has no end")


def find_item(s, pattern, what):
    m = re.search(pattern, s, re.M)
    if not m:
        raise SystemExit(f"not found: {what}")
    start = leading(s, m.start())
    kind = re.search(r"\b(fn|enum|struct|const|type|static)\b", m.group(0)).group(1)
    end = item_end(s, m.end(), kind)
    if end < len(s) and s[end] == "\n":
        end += 1
    return start, end


def publish(text, indent):
    """Private `fn`/`enum`/`struct`/`const`/`type` at `indent` become `pub(super)`."""
    return re.sub(
        rf"^({indent})(?=(?:async |unsafe |const )?(fn|enum|struct|const|type|static) )",
        r"\1pub(super) ",
        text,
        count=1,
        flags=re.M,
    )


def main():
    plan = json.loads(Path(sys.argv[1]).read_text())
    dry = "--dry-run" in sys.argv
    path = Path(plan["file"])
    s = path.read_text()
    ty = plan["type"]
    out_dir = path.parent if path.name in ("mod.rs", "lib.rs", "main.rs") else path.parent / path.stem
    decls = []
    outputs = []  # written only once every method and item is found
    for name, spec in plan["modules"].items():
        methods, items = [], []
        for meth in spec.get("methods", []):
            start, end = find_item(s, rf"^    (?:pub(?:\([a-z]+\))? )?fn {meth}\b", f"method {meth}")
            text = s[start:end]
            methods.append(publish(text, "    ") if not re.search(r"^\s*pub", text.split("fn ")[0].splitlines()[-1]) else text)
            s = s[:start] + s[end:]
        for item in spec.get("items", []):
            start, end = find_item(
                s, rf"^(?:pub(?:\([a-z]+\))? )?(?:fn|enum|struct|const|type|static) {item}\b", f"item {item}"
            )
            text = s[start:end]
            items.append(text if re.search(r"^pub", text, re.M) else publish(text, ""))
            s = s[:start] + s[end:]
        impls = []
        for head in spec.get("impls", []):
            i = s.index(head)
            start = leading(s, i)
            end = block_end(s, s.index("{", i))
            if end < len(s) and s[end] == "\n":
                end += 1
            block = s[start:end]
            # A trait impl's methods take the trait's visibility; only inherent ones are published.
            if not re.match(r"impl(<[^>]*>)? \S+ for ", head):
                block = re.sub(r"^    fn ", "    pub(super) fn ", block, flags=re.M)
            impls.append(block)
            s = s[:start] + s[end:]
        body = f"//! {spec['doc']}\n\nuse super::*;\n\n"
        body += "\n".join(i.rstrip("\n") + "\n" for i in items)
        if methods:
            body += ("\n" if items else "") + f"impl {ty} {{\n" + "\n".join(m.rstrip("\n") + "\n" for m in methods) + "}\n"
        for block in impls:
            body += "\n" + block.rstrip("\n") + "\n"
        target = out_dir / f"{name}.rs"
        print(f"{target}: {len(methods)} methods, {len(items)} items, {len(impls)} impl blocks")
        outputs.append((target, body))
        # Re-export each item with its own visibility, so its path from outside stays the same.
        by_vis = {}
        for i in items:
            m = re.search(r"^(pub(?:\([a-z]+\))? )?(?:fn|enum|struct|const|type|static) (\w+)", i, re.M)
            vis = (m.group(1) or "") if m.group(1) != "pub(super) " else ""
            by_vis.setdefault(vis, []).append(m.group(2))
        decls.append((name, by_vis))
    lines = "".join(f"mod {n};\n" for n, _ in decls)
    lines += "".join(
        f"{vis}use {n}::{{{', '.join(sorted(ns))}}};\n" for n, by_vis in decls for vis, ns in sorted(by_vis.items())
    )
    s = re.sub(r"(\A(?://![^\n]*\n)*\n?)", lambda m: m.group(1) + lines + "\n", s, count=1)
    s = re.sub(r"\n{3,}", "\n\n", s)
    if not dry:
        out_dir.mkdir(parents=True, exist_ok=True)
        for target, body in outputs:
            target.write_text(body)
        path.write_text(s)


if __name__ == "__main__":
    main()
