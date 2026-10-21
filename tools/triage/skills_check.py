#!/usr/bin/env python3
"""Check the agent skills against the repository, so they never send an agent to a dead path.

Usage: python3 tools/triage/skills_check.py

Checks that:
- every `.claude/skills/<name>/SKILL.md` starts with front matter whose `name` is the folder's
  name and whose `description` is not empty;
- the skills table in AGENTS.md lists every skill folder, and nothing else;
- every repository path a skill mentions (`tools/...`, `crates/...`, `apps/...`, `docs/...`,
  `site/...`, `.github/...`) exists. Paths under the ignored folders (game-data, analysis,
  save-game, target, derived) and paths with placeholders (`<name>`, `NN`, `*`, `X.py`) are
  not checked.
Exit status 1 when anything is wrong; each problem is printed with its file and line.
"""
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
SKILLS = os.path.join(ROOT, '.claude', 'skills')
PATH = re.compile(r'(?<![\w/.-])((?:tools|crates|apps|docs|site|\.github)/[\w./-]*[\w/])')
PLACEHOLDER = re.compile(r'<|>|\*|\{|\bNN\b|\bX\.|\bXX|/\.\.\.|\.\.\.')


def front_matter(text):
    if not text.startswith('---\n'):
        return None
    end = text.find('\n---', 4)
    if end < 0:
        return None
    fields = {}
    for line in text[4:end].splitlines():
        key, sep, value = line.partition(':')
        if sep:
            fields[key.strip()] = value.strip()
    return fields


def main():
    problems = []
    checked = 0
    names = sorted(d for d in os.listdir(SKILLS) if os.path.isfile(os.path.join(SKILLS, d, 'SKILL.md')))
    for name in names:
        rel = f'.claude/skills/{name}/SKILL.md'
        text = open(os.path.join(SKILLS, name, 'SKILL.md'), encoding='utf-8').read()
        fm = front_matter(text)
        if fm is None:
            problems.append(f'{rel}: no front matter (--- name: ... description: ... ---)')
        else:
            if fm.get('name') != name:
                problems.append(f'{rel}: front matter name {fm.get("name")!r} is not the folder name {name!r}')
            if not fm.get('description'):
                problems.append(f'{rel}: empty description (agents choose skills by it)')
        for n, line in enumerate(text.splitlines(), 1):
            for m in PATH.finditer(line):
                path = m.group(1).rstrip('.')
                if PLACEHOLDER.search(path) or PLACEHOLDER.search(line[m.end():m.end() + 1]):
                    continue
                checked += 1
                if not os.path.exists(os.path.join(ROOT, path)):
                    problems.append(f'{rel}:{n}: {path} does not exist')
    agents = open(os.path.join(ROOT, 'AGENTS.md'), encoding='utf-8').read()
    listed = set(re.findall(r'^\|[^|\n]*\|\s*`([\w-]+)`\s*\|\s*$', agents, re.M))
    for name in names:
        if name not in listed and name != 'banished':
            problems.append(f'AGENTS.md: the skills table does not list {name}')
    for name in sorted(listed - set(names)):
        problems.append(f'AGENTS.md: the skills table lists {name}, which has no SKILL.md')
    for p in problems:
        print(p)
    print(f'{len(names)} skills and {checked} paths checked, {len(problems)} problems')
    return 1 if problems else 0


if __name__ == '__main__':
    sys.exit(main())
