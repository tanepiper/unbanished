"""The steps of an Un-Banished release that change files, for .github/workflows/release.yml.

Usage (from the repository root):
  python3 tools/release/release.py check VERSION        # can VERSION be released now?
  python3 tools/release/release.py changelog VERSION DATE  # Unreleased -> [VERSION] - DATE
  python3 tools/release/release.py notes VERSION        # the release notes, to standard output
  python3 tools/release/release.py title VERSION        # "Un-Banished VERSION: NAME" if a blog post names it
  python3 tools/release/release.py bump NEXT            # start NEXT: Cargo.toml, Cargo.lock, changelog

Each step only edits files; the workflow commits, tags and publishes. Run them locally on a
scratch copy to see what a release will do (`--dry-run` prints instead of writing).
AGENTS.md ("Versions") describes the release process.
"""
import re
import subprocess
import sys
from pathlib import Path

REPO = "https://github.com/tanepiper/unbanished"
SITE = "https://tanepiper.github.io/unbanished"
VERSION = re.compile(r"^\d+\.\d+\.\d+$")

changelog_path = Path("CHANGELOG.md")
cargo_path = Path("Cargo.toml")
lock_path = Path("Cargo.lock")
dry = "--dry-run" in sys.argv


def fail(message):
    print(f"release: {message}", file=sys.stderr)
    sys.exit(1)


def write(path, text):
    if dry:
        print(f"--- {path} (not written) ---\n{text}")
    else:
        path.write_text(text)


def workspace_version():
    cargo = cargo_path.read_text()
    m = re.search(r'\[workspace\.package\][^\[]*?\nversion = "([^"]+)"', cargo)
    if not m:
        fail("no version in [workspace.package] of Cargo.toml")
    return m.group(1)


def sections(text):
    """The changelog's `## [...]` sections: (title, start, end) with the body between."""
    heads = list(re.finditer(r"^## \[([^\]]+)\][^\n]*\n", text, re.M))
    links = re.search(r"^\[Unreleased\]:", text, re.M)
    end = links.start() if links else len(text)
    return [(h.group(1), h.start(), heads[i + 1].start() if i + 1 < len(heads) else end, h.end())
            for i, h in enumerate(heads)]


def section_body(text, title):
    for t, _, end, body in sections(text):
        if t == title:
            return text[body:end].strip("\n")
    return None


def blog_post(version):
    """The blog post for a version (site/blog/*.md with `version: VERSION`): (slug, name)."""
    for post in sorted(Path("site/blog").glob("*.md")):
        text = post.read_text()
        if re.search(rf"^version: {re.escape(version)}\s*$", text, re.M):
            name = re.search(r"^name: (.+)$", text, re.M)
            return post.stem, name.group(1).strip() if name else None
    return None, None


def check(version):
    if not VERSION.match(version):
        fail(f"{version!r} is not X.Y.Z")
    if workspace_version() != version:
        fail(f"Cargo.toml says {workspace_version()}, not {version}")
    text = changelog_path.read_text()
    body = section_body(text, "Unreleased")
    if body is None or not re.search(r"^- ", body, re.M):
        fail("the changelog's Unreleased section has no entries")
    if f"## [{version}]" in text:
        fail(f"the changelog already has a section for {version}")
    tags = subprocess.run(["git", "tag", "--list", f"v{version}"], capture_output=True, text=True).stdout
    if tags.strip():
        fail(f"tag v{version} already exists")
    print(f"release: {version} can be released")


def changelog(version, date):
    text = changelog_path.read_text()
    if "## [Unreleased]" not in text:
        fail("no Unreleased section")
    # The development note ("Version X in development.") does not belong in the release.
    text = re.sub(r"(## \[Unreleased\]\n\n)Version [\d.]+ in development\.[^\n]*(\n[^\n#-][^\n]*)*\n\n",
                  r"\1", text, count=1)
    text = text.replace("## [Unreleased]", f"## [Unreleased]\n\n## [{version}] - {date}", 1)
    previous = re.search(r"^\[Unreleased\]: .*/compare/v([\d.]+)\.\.\.HEAD$", text, re.M)
    if not previous:
        fail("no [Unreleased] compare link at the end of the changelog")
    text = text.replace(
        previous.group(0),
        f"[Unreleased]: {REPO}/compare/v{version}...HEAD\n"
        f"[{version}]: {REPO}/compare/v{previous.group(1)}...v{version}",
    )
    write(changelog_path, text)


def notes(version):
    body = section_body(changelog_path.read_text(), version)
    if body is None:
        fail(f"no changelog section for {version}")
    # Links relative to the repository (docs/...) become links on GitHub.
    body = re.sub(r"\]\((?!https?:|#)([^)]+)\)", rf"]({REPO}/blob/v{version}/\1)", body)
    slug, name = blog_post(version)
    lead = ""
    if slug:
        lead = (f"**{name}**: read the story of this release, with pictures and videos, on the "
                f"[Un-Banished blog]({SITE}/blog/{slug}).\n\n")
    print(lead + body + f"\n\n**Full changelog:** {REPO}/blob/v{version}/CHANGELOG.md")


def title(version):
    _, name = blog_post(version)
    print(f"Un-Banished {version}" + (f": {name}" if name else ""))


def bump(next_version):
    if not VERSION.match(next_version):
        fail(f"{next_version!r} is not X.Y.Z")
    old = workspace_version()
    cargo = cargo_path.read_text()
    cargo = re.sub(r'(\[workspace\.package\][^\[]*?\nversion = ")[^"]+(")',
                   rf"\g<1>{next_version}\2", cargo, count=1)
    write(cargo_path, cargo)
    # Workspace packages are the lock entries without a `source`.
    blocks = lock_path.read_text().split("\n[[package]]\n")
    changed = 0
    for i, block in enumerate(blocks):
        if "\nsource = " not in block and f'\nversion = "{old}"' in block and block.startswith("name = "):
            blocks[i] = block.replace(f'\nversion = "{old}"', f'\nversion = "{next_version}"', 1)
            changed += 1
    write(lock_path, "\n[[package]]\n".join(blocks))
    text = changelog_path.read_text()
    text = text.replace("## [Unreleased]\n",
                        f"## [Unreleased]\n\nVersion {next_version} in development.\n", 1)
    write(changelog_path, text)
    print(f"release: {old} -> {next_version} ({changed} workspace packages in Cargo.lock)", file=sys.stderr)


def main():
    args = [a for a in sys.argv[1:] if a != "--dry-run"]
    if not args:
        fail(__doc__)
    step, rest = args[0], args[1:]
    steps = {"check": (check, 1), "changelog": (changelog, 2), "notes": (notes, 1),
             "title": (title, 1), "bump": (bump, 1)}
    if step not in steps or len(rest) != steps[step][1]:
        fail(__doc__)
    steps[step][0](*rest)


if __name__ == "__main__":
    main()
