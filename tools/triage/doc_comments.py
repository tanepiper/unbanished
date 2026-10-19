#!/usr/bin/env python3
"""Keep the evidence of a port out of the rendered Rust documentation, without losing it.

The project's doc-comment style (AGENTS.md rule 2): the text of a `///` or `//!` comment is
plain English for readers of the API documentation. A ported item ends with one line,
`Original: `FUN_...``, that names the original functions. Field offsets (`+0x48`), vtables and
`DAT_` constants go in ordinary `//` comments next to the item: they stay in the source, where
fnindex.py and coverage.py still find them, but rustdoc does not show them.

    python3 tools/triage/doc_comments.py check [--all] [PATH...]   # doc lines rustdoc shows with evidence
    python3 tools/triage/doc_comments.py fix [PATH...]       # move the evidence that is safe to move

`fix` moves only what it can move without changing the meaning:
- parentheses that hold only evidence, for example "(`FUN_14011fd30`)" or
  "(vtable `0x1401689a0`, constructor `FUN_140115040`)";
- a leading "`FUN_...`:", "`+0x48`:", "`DAT_...`:" or "Port of FUN_...:" before the text.
Everything it removes is kept: functions in the `Original:` line, the rest in a `//` line.
Code blocks, tables and lines with "Deviation" or "not ported" are left as they are. `check`
then lists what is left for a person to rewrite. Both commands only touch what rustdoc renders
(module docs and the docs of public items). Paths default to crates/ (the libraries, whose API
documentation is published); tests, examples and binaries are skipped.
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
WIDTH = 100

# One piece of evidence: a function, a constant, a code or data address, or a field offset.
ADDR = r"(?:FUN_[0-9a-f]{8,9}|DAT_[0-9a-f]{8,9}|0x1[48][0-9a-f]{7,8}|\+0x[0-9a-f]+)"
EV = re.compile(
    r"`?(?:Runtime(?:-steam-x64\.dll)?\s+|Application(?:-steam-x64\.exe)?\s+)?"
    + ADDR
    + r"(?:\s*(?:\.\.|/|–|-)\s*`?(?:\+?0x[0-9a-f]+|FUN_[0-9a-f]{8,9})`?)*(?:'s)?`?"
)
FUNC = re.compile(r"FUN_[0-9a-f]{8,9}|0x1[48]0[0-9a-f]{6}(?![0-9a-f])")
# Words that only label a piece of evidence ("vtable `0x...`", "constructor `FUN_...`").
LABELS = {
    "vtable", "constructor", "factory", "serializer", "serialiser", "states", "state", "slot",
    "update", "start", "enter", "exit", "runtime", "application", "and", "or", "then", "also",
    "at", "in", "of", "the", "its", "see", "via", "thunk", "dll", "exe", "table", "ctor",
    "callback", "caller", "callers", "handler", "registration", "registered",
}
DOC = re.compile(r"^(\s*)(///|//!)( ?)(.*)$")


def is_evidence_only(part: str) -> bool:
    """A comma-separated piece of a parenthesis that holds a reference and nothing else."""
    if not EV.search(part):
        return False
    rest = EV.sub(" ", part)
    words = re.findall(r"[A-Za-z][A-Za-z_:]*", rest)
    return all(w.lower() in LABELS for w in words)


def split_evidence(text: str):
    """Return (text, functions, other evidence) with the safe-to-move evidence taken out."""
    funcs, other = [], []

    def keep(piece: str):
        found = FUNC.findall(piece)
        if found and not re.search(r"vtable", piece):
            funcs.extend(found)
            rest = FUNC.sub("", piece)
            if re.search(r"\+0x|DAT_", rest):
                other.append(piece.strip(" `,;"))
        else:
            other.append(piece.strip(" ,;"))

    def paren(m):
        inner = m.group(1)
        parts = re.split(r"\s*[,;]\s*", inner)
        kept = []
        for p in parts:
            if is_evidence_only(p):
                keep(p)
            else:
                kept.append(p)
        if len(kept) == len(parts):
            return m.group(0)
        return f" ({', '.join(kept)})" if kept else ""

    before = len(funcs)
    text = re.sub(r"\s*\(([^()]*)\)", paren, text)
    # "`Math::Vector3::Length`: text" after its address was taken out: the name is the original's.
    m = re.match(r"^`([A-Za-z_][\w:]*(?:\([^`]*\))?)`\s*(?::\s+|\.?$)", text)
    if m and len(funcs) > before and ("::" in m.group(1) or "(" in m.group(1)):
        funcs.insert(before, "name:" + m.group(1))
        text = text[m.end():]
    # A leading "`FUN_x`:", "+0x48:", "Port of FUN_x:" or "Port of X (FUN_x):" prefix.
    m = re.match(r"^(?:Port of\s+)?((?:" + EV.pattern + r")(?:\s*(?:,|/|and)\s*" + EV.pattern + r")*)\s*:\s+", text)
    m2 = re.match(r"^(?:Port of\s+)?`?(FUN_[0-9a-f]{8,9})`?'s\s+([^:.`]{1,40})[:.]\s+", text)
    m3 = re.match(r"^(?:Port of\s+)?((?:`?FUN_[0-9a-f]{8,9}`?(?:\([^)]*\))?\s*(?:→|->|\+|/|and)\s*(?:[a-z]+\s+)?)+`?FUN_[0-9a-f]{8,9}`?(?:\([^)]*\))?)\s*:\s+", text)
    m4 = re.match(r"^Port of\s+`?(FUN_[0-9a-f]{8,9})`?\.\s+", text)
    if m2:
        funcs.append(f"name:{m2.group(1)}` ({m2.group(2).strip()})`")
        text = text[m2.end():]
    elif m3:
        funcs.extend(FUNC.findall(m3.group(1)))
        text = text[m3.end():]
    elif m4:
        funcs.append(m4.group(1))
        text = text[m4.end():]
    elif m:
        keep(m.group(1))
        text = text[m.end():]
    else:
        m = re.match(r"^Port of\s+(`?[A-Za-z_:]+(?:\([^)]*\))?`?(?:'s [a-z ]+)?)\s*[:.]\s+", text)
        if m:
            other.append("Port of " + m.group(1))
            text = text[m.end():]
    # Offsets and constants inside a sentence: "BuildComponent `+0x48` progress".
    def offset(m):
        other.append(m.group(0).strip())
        return ""
    OFF = r"(?:DAT_[0-9a-f]{8,9}(?:\s*\+\s*0x[0-9a-f]+)?|\+0x[0-9a-f]+(?:\s*(?:\.\.|/)\s*\+?0x[0-9a-f]*)?)"

    def span(m):
        inner = m.group(1)
        if re.fullmatch(r"\s*(?:" + OFF + r"|\+[0-9a-f]{1,3})\s*", inner):
            other.append(inner.strip())
            return ""
        if re.search(OFF, inner):
            other.append(inner.strip())
            inner = re.sub(r"\s*" + OFF, "", inner).strip()
            return f" `{inner}`" if inner else ""
        return m.group(0)

    text = re.sub(r"\s*`([^`]*)`", lambda m: (" " if m.group(0)[:1].isspace() else "") + span(m).lstrip() if span(m) != m.group(0) else m.group(0), text)
    text = re.sub(r"\s*" + OFF, offset, text)
    text = re.sub(r"^(?:Slot|Field|Byte|Offset)\s*[:,]\s+", "", text)
    text = re.sub(r"\s+([,.;:)])", r"\1", text).replace("( ", "(").replace("()", "")
    text = re.sub(r"(?:,\s*){2,}", ", ", text)
    text = re.sub(r",\s*([.;:)])", r"\1", text)
    text = re.sub(r"\(,\s*", "(", text)
    text = re.sub(r"\s+(?:and|or)\s*([.;:)])", r"\1", text)
    text = re.sub(r"^the original\b", "The original", text)
    if text[:1].islower() and text[:1].isalpha():
        text = text[0].upper() + text[1:]
    return text.strip(), funcs, other


def wrap(prefix: str, first: str, hang: str, text: str) -> list:
    """Wrap text to WIDTH; first-line marker `first`, continuation indent `hang`."""
    words, lines, line = text.split(), [], first
    for w in words:
        candidate = (line + w) if (line.endswith(" ") or not line.strip()) else (line + " " + w)
        if len(prefix) + len(candidate) > WIDTH and line.strip():
            lines.append(line.rstrip())
            line = hang + w
        else:
            line = candidate if line.strip() or line else w
    if line.strip():
        lines.append(line.rstrip())
    return [prefix + l if l else prefix.rstrip() for l in lines]


def units(body: list):
    """Split a doc block's text lines into paragraphs and list items."""
    out, cur = [], None
    for line in body:
        if not line.strip():
            if cur:
                out.append(cur)
            out.append(("blank", "", ""))
            cur = None
            continue
        m = re.match(r"^(\s*)([-*]|\d+\.)\s+(.*)$", line)
        if m:
            if cur:
                out.append(cur)
            marker = m.group(1) + m.group(2) + " "
            cur = ["item", marker, m.group(3)]
        elif cur and (line.startswith(" ") or cur[0] == "para"):
            cur[2] += " " + line.strip()
        else:
            if cur:
                out.append(cur)
            cur = ["para", "", line.strip()]
    if cur:
        out.append(cur)
    return out


def fix_block(indent: str, kind: str, body: list):
    """Rewrite one doc block. Returns the new lines, or None to leave it unchanged."""
    joined = "\n".join(body)
    if "```" in joined or re.search(r"Deviation|not ported", joined) or not EV.search(joined):
        return None
    if any(l.lstrip().startswith("|") for l in body):
        return None
    prefix = f"{indent}{kind} "
    out, funcs, other = [], [], []
    for u in units(body):
        if u[0] == "blank":
            out.append(prefix.rstrip())
            continue
        text, f, o = split_evidence(u[2])
        funcs += f
        other += o
        if not text:
            continue
        marker = u[1]
        out += wrap(prefix, marker, " " * len(marker), text)
    while out and out[-1] == prefix.rstrip():
        out.pop()
    if not funcs and not other:
        return None
    seen = []
    for f in funcs:
        if f.startswith("name:"):
            f = f"`{f[5:]}`".replace("``", "")
        elif f.startswith("0x18"):
            f = f"Runtime `{f}`"
        else:
            f = f"`{f}`"
        if f not in seen:
            seen.append(f)
    if seen:
        if out:
            out.append(prefix.rstrip())
        out.append(prefix + "Original: " + ", ".join(seen) + ".")  # one line, never wrapped
    comment = []
    for o in other:
        o = o.replace("`", "")
        if o and o not in comment:
            comment.append(o)
    evidence = wrap(f"{indent}// ", "", "", "; ".join(comment)) if comment else []
    if kind == "///":
        return evidence + out
    return out + evidence


def process(path: Path, write: bool) -> int:
    src = path.read_text().split("\n")
    out, i, changed = [], 0, 0
    while i < len(src):
        m = DOC.match(src[i])
        if not m:
            out.append(src[i])
            i += 1
            continue
        indent, kind = m.group(1), m.group(2)
        body = []
        while i < len(src):
            m2 = DOC.match(src[i])
            if not m2 or m2.group(1) != indent or m2.group(2) != kind:
                break
            body.append(m2.group(4))
            i += 1
        rendered_block = kind == "//!" or rendered(src, i - 1)
        new = fix_block(indent, kind, body) if write and rendered_block else None
        if new is None:
            out += [f"{indent}{kind}{' ' if b else ''}{b}" for b in body]
        else:
            out += new
            changed += 1
    if write and changed:
        path.write_text("\n".join(out))
    return changed


def rendered(lines, n) -> bool:
    """Whether rustdoc shows the doc line at index n: a module doc, or the doc of a public item."""
    if DOC.match(lines[n]).group(2) == "//!":
        return True
    for line in lines[n + 1:]:
        t = line.strip()
        if not t or t.startswith("//") or t.startswith("#["):
            continue
        if re.match(r"pub(?!\()\s", t):
            return True
        # A variant of an enum, or a field of a variant: shown when the enum is public.
        if re.match(r"[A-Za-z_]\w*\s*([:({,]|$)", t):
            return in_public_enum(lines, n)
        return False
    return False


def in_public_enum(lines, n) -> bool:
    """Whether line n lies inside a `pub enum` block."""
    depth = 0
    for line in reversed(lines[:n]):
        depth += line.count("}") - line.count("{")
        if depth < 0:
            t = line.strip()
            if re.match(r"(pub\s+)?enum\b", t):
                return t.startswith("pub ") and not t.startswith("pub(")
            if re.match(r"[A-Z]\w*\s*\{", t):  # a struct variant: keep looking for its enum
                depth = 0
                continue
            return False
    return False


def check(paths, everything=False) -> int:
    count = 0
    for p in paths:
        lines = p.read_text().split("\n")
        in_original = False
        for n, line in enumerate(lines, 1):
            m = DOC.match(line)
            if not m:
                in_original = False
                continue
            if re.search(r"Deviation|not ported", line):
                continue
            if not everything and not rendered(lines, n - 1):
                continue
            text = m.group(4)
            if text.startswith("Original:"):
                in_original = True
                continue
            if in_original and text.strip():
                continue  # the Original paragraph goes on
            in_original = False
            if re.search(ADDR, text):
                count += 1
                name = p.relative_to(ROOT) if p.is_relative_to(ROOT) else p
                print(f"{name}:{n}: {text.strip()}")
    return count


def files(args):
    roots = [Path(a) for a in args] or [ROOT / "crates"]
    for r in roots:
        r = r if r.is_absolute() else Path.cwd() / r
        if r.is_file():
            yield r
            continue
        for p in sorted(r.rglob("*.rs")):
            # Tests, examples and binaries are not part of the API documentation.
            if not {"tests", "examples", "bin", "target"} & set(p.relative_to(r).parts):
                yield p


if __name__ == "__main__":
    if len(sys.argv) < 2 or sys.argv[1] not in ("check", "fix"):
        print(__doc__)
        sys.exit(2)
    paths = list(files([a for a in sys.argv[2:] if not a.startswith("--")]))
    if sys.argv[1] == "fix":
        n = sum(process(p, True) for p in paths)
        print(f"rewrote {n} doc blocks")
    left = check(paths, "--all" in sys.argv) if sys.argv[1] == "check" else None
    if left is not None:
        print(f"{left} doc lines still show evidence", file=sys.stderr)
        sys.exit(1 if left else 0)
