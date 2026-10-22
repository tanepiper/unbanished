"""Split a Rust file of free functions (usually tests) into topic files.

Usage: python3 tools/refactor/split_by_topic.py FILE.rs PLAN.json

PLAN.json: {"topics": {"TOPIC": ["fn_name", ...], ...}, "doc": {"TOPIC": "//! One line.", ...}}

FILE.rs becomes FILE/mod.rs, with the file's header (its `use` lines) and every function the plan
does not name (the shared helpers), followed by `mod TOPIC;` for each topic; FILE/TOPIC.rs holds
the doc line, `use super::*;` and the functions named for it. Each function moves unchanged with
its comments and attributes. Children see the parent's private helpers and imports through
`use super::*`. Code is moved, never changed: the compiler and the tests are the check. Used for
the placement and game-loop tests (2026-10-22).
"""
import json, re, sys
from pathlib import Path
src, plan = Path(sys.argv[1]), json.load(open(sys.argv[2]))
lines = src.read_text().split('\n')
starts = [i for i, l in enumerate(lines) if re.match(r'(pub(\(\w+\))? )?fn \w', l)]
chunks = []
for k in starts:
    s = k
    while s > 0 and (lines[s-1].startswith(('#[', '///', '//')) ):
        s -= 1
    chunks.append([s, k, re.match(r'(?:pub(?:\(\w+\))? )?fn (\w+)', lines[k]).group(1)])
for n, c in enumerate(chunks):
    c.append(chunks[n+1][0] if n + 1 < len(chunks) else len(lines))
header = lines[:chunks[0][0]]
topic_of = {name: t for t, names in plan['topics'].items() for name in names}
helpers, topics = [], {t: [] for t in plan['topics']}
for s, k, name, e in chunks:
    body = lines[s:e]
    (topics[topic_of[name]] if name in topic_of else helpers).append(body)
missing = set(topic_of) - {c[2] for c in chunks}
assert not missing, missing
out = src.with_suffix('')
out.mkdir()
def join(parts): return '\n'.join(l for p in parts for l in p).rstrip('\n') + '\n'
mods = ''.join(f'mod {t};\n' for t in plan['topics'])
(out / 'mod.rs').write_text('\n'.join(header).rstrip('\n') + '\n\n' + mods + '\n' + join(helpers))
for t, parts in topics.items():
    (out / f'{t}.rs').write_text(plan['doc'][t] + '\n\nuse super::*;\n\n' + join(parts))
src.unlink()
print({t: len(p) for t, p in topics.items()}, 'helpers', len(helpers))
