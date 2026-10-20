"""Rename the code from `banished` to `unbanished` (docs/ARCHITECTURE-PLAN.md, phase R).

Usage: python3 tools/refactor/rename_unbanished.py [--dry-run]

Renames the workspace crates (`banished-core` -> `unbanished-core`, ...), their Rust paths
(`banished_game::` -> `unbanished_game::`) and the programs (`banished` -> `unbanished`,
`banished-mods`, `banished-cli`, `banished-relay`), in every tracked text file. It never touches
the names that hold players' data (`banished-wasm`: the settings folder, the browser's
database and its settings key), the original game's names (`Banished`, `BanishedKit`), the
`BANISHED_*` environment variables (still read, after `UNBANISHED_*`), released history (the
blog, CHANGELOG.md), or generated files (Cargo.lock, docs/DEVIATIONS.md).
"""
import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
CRATES = ["banishedinfo", "core", "terrain", "path", "game", "formats", "ui", "gfx", "audio",
          "wma", "mods", "nexus", "net", "app", "cli", "modmanager", "relay", "viewer"]
SKIP_PREFIXES = ("site/blog/", "game-data/", "analysis/", "save-game/", "site/public/")
SKIP_FILES = {"CHANGELOG.md", "Cargo.lock", "docs/DEVIATIONS.md",
              "tools/refactor/rename_unbanished.py"}
TEXT = {".rs", ".toml", ".md", ".sh", ".py", ".mjs", ".js", ".ts", ".mts", ".vue", ".json",
        ".yml", ".yaml", ".html", ".css", ".txt", ".wgsl", ".tsv"}

names = "|".join(CRATES)
RULES = [
    # Crates, as package names and dependency keys: banished-core -> unbanished-core.
    (re.compile(rf"(?<![\w-])banished-({names})(?![\w-])"), r"unbanished-\1"),
    # Their Rust paths: banished_core -> unbanished_core.
    (re.compile(rf"(?<![\w])banished_({names})(?!\w)"), r"unbanished_\1"),
    # The game's own module paths in log targets: banished::loops -> unbanished::loops.
    (re.compile(r"(?<![\w-])banished::"), "unbanished::"),
    # The game program in paths: target/release/banished -> target/release/unbanished.
    (re.compile(r"((?:target|target/profiling|target/web)/release/)banished(?![\w-])"), r"\1unbanished"),
    (re.compile(r"(?<![\w-])banished\.exe"), "unbanished.exe"),
    # The game program as a command: `banished`, banished --version, banished PATH, banished [WINDATA].
    (re.compile(r"`banished`"), "`unbanished`"),
    (re.compile(r"(?<![\w/.\\-])banished(?=\s+(?:--|-V\b|\[|PATH\b|WINDATA\b|\"|/|\.\.\.|…|path/))"), "unbanished"),
    (re.compile(r"`banished "), "`unbanished "),
    (re.compile(r"(--bin\s+)banished(?![\w-])"), r"\1unbanished"),
]


def tracked():
    out = subprocess.run(["git", "ls-files"], cwd=REPO, capture_output=True, text=True, check=True).stdout
    for name in out.split("\n"):
        if not name or name in SKIP_FILES or name.startswith(SKIP_PREFIXES):
            continue
        p = REPO / name
        if p.suffix in TEXT or p.name in ("Dockerfile", "Trunk.toml"):
            yield name, p


def main():
    dry = "--dry-run" in sys.argv
    changed = 0
    for name, p in tracked():
        try:
            s = p.read_text()
        except (UnicodeDecodeError, FileNotFoundError):
            continue
        t = s
        for pattern, repl in RULES:
            t = pattern.sub(repl, t)
        # The game's binary and the trunk link name.
        if name == "apps/game/Cargo.toml":
            t = t.replace('name = "banished"\n', 'name = "unbanished"\n')
        if name == "apps/game/index.html":
            t = t.replace('data-bin="banished"', 'data-bin="unbanished"')
        if t != s:
            changed += 1
            print(name)
            if not dry:
                p.write_text(t)
    print(f"{changed} files {'would change' if dry else 'changed'}")


if __name__ == "__main__":
    main()
