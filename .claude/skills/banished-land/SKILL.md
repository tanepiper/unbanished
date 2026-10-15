---
name: banished-land
description: Land Banished reverse-engineering or port work — write findings into docs/ in the project's evidence style, update GAME-LOOP-CHECKLIST.md, then commit only your own changes (never game data or decompiled code) and push, safely alongside other concurrent sessions. Use at the end of every trace, format, port or shader slice.
---

# Document, commit and push

## 1. Document

- **Where**: extend the topic doc if one exists (`grep -rln <address|resource> docs/`); otherwise
  create `docs/<TOPIC-IN-CAPS>.md`. Sequential game-loop traces continue as
  `docs/GAME-LOOP-STEP-NN.md` (next free number) and are linked from the checklist.
- **Style** (match existing docs):
  - opening paragraph: what this covers, binary/build, evidence type (static Ghidra 12.1.4
    pseudocode, disassembly, emulation, decoded resources) and what was *not* observed;
  - prose and tables keyed by function address (`FUN_1400be770`) and field offsets (`+0x44`);
  - every claim is observed, *inferred* (say so), or listed under **Open questions**;
  - links to related docs; no copied pseudocode, disassembly or resource dumps.
- **Checklist**: tick or add items in `docs/GAME-LOOP-CHECKLIST.md` (both "reverse-engineering" and
  "port" sections) and bump its `Updated` date. Update `README.md` status/crate table if a new
  capability or command exists.

## 2. Check what you are about to commit

```bash
git status --short
git diff --stat
```
Never stage: anything under `game-data/` or `analysis/`, `*.pkg *.crs *.rsc *.str *.sav *.exe *.dll`,
`.venv/`, `target/`, `dist/`, PNG captures, emulator dumps, shader disassembly. `.gitignore` covers
most — don't force-add (`-f`) past it.

## 3. Stage only your own work (concurrent sessions)

Other sessions edit this repo at the same time. Never `git add -A` / `git add .` / `git commit -a`.

- Whole files you own: `git add path/one path/two`.
- A shared file with someone else's unrelated edits: stage only your hunks. Non-interactive way —
  build the staged version and write it straight into the index:
  ```bash
  git show HEAD:apps/game/src/main.rs > "$TMP/main.rs"   # then apply only your edits to it
  blob=$(git hash-object -w "$TMP/main.rs")
  git update-index --cacheinfo 100644,$blob,apps/game/src/main.rs
  ```
  (`git add -p` is interactive and unavailable.) If your change can't be separated, ask before
  committing someone else's work.
- `git diff --cached --stat` to confirm, and make sure the staged tree builds if you split hunks
  (`git stash --keep-index` is unsafe with concurrent writers — prefer building a worktree:
  `git worktree add "$TMP/check" HEAD` then apply `git diff --cached` there and `cargo check`).

## 4. Commit and push

Message: imperative subject ≤ 72 chars naming the feature/trace (`Port toolbar root level`,
`Trace status bar counters`), body with what was traced/ported and how it was verified. End with:

```
Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
```

```bash
git commit -F "$TMP/msg.txt"
git show --stat HEAD            # confirm only your files went in
git pull --rebase origin main && git push origin main
```
If the rebase conflicts in files you didn't touch, stop and report rather than resolving someone
else's work.

## 5. Report

One or two lines: commit hash, what landed, what's verified, and the next open item from the
checklist.

## Deviation ledger

Mark every intentional difference from the original in a code comment ("Deviation: …",
"not ported"), then run `python3 tools/triage/deviations.py` to refresh `docs/DEVIATIONS.md`
and commit it with the change.
