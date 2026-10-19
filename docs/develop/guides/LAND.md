# 7. Document and land a change

The last step writes down what you found, then commits and pushes the change. Do not skip the
documentation: a finding that is not written down will be worked out again by the next person.

## 1. Document

**Where.** Extend the topic document if one exists:

```bash
grep -rln "FUN_1400be770\|Toolbar" docs/
```

If there is none, create `docs/<TOPIC-IN-CAPITALS>.md`. Traces of the game loop, in order,
continue as `docs/GAME-LOOP-STEP-NN.md`, with the next free number, linked from the checklist.

**Style.** Match the existing documents:

- Start with a paragraph that says what the document covers, which program and build, the kind
  of evidence (static Ghidra pseudocode, disassembly, emulation, decoded resources) and what was
  *not* observed.
- Use prose and tables, keyed by function address (`FUN_1400be770`) and field offset (`+0x44`).
- Mark every claim: observed, *inferred*, or listed under **Open questions**.
- Link to related documents.
- Never include copied pseudocode, disassembly or resource dumps.
- Write plain English: short sentences, one idea in each.

**Checklist.** Tick or add the items in [GAME-LOOP-CHECKLIST.md](../../GAME-LOOP-CHECKLIST.md),
in both the reverse-engineering and the port sections, and update its date. If the change adds
a capability or a command, update the README's status or the crate's README. If a player would
notice the change, add a line to the `Unreleased` section of [CHANGELOG.md](../../../CHANGELOG.md).
Changes to controls, file locations or options also go in the [user guide](../../USER-GUIDE.md).

**Doc comments.** Check that the rendered API documentation shows no addresses or offsets (see
[the doc-comment style](../API.md#the-doc-comment-style)):

```bash
python3 tools/triage/doc_comments.py check
```

**Deviations.** If you added a `Deviation:` or `not ported` comment, update the list:

```bash
python3 tools/triage/deviations.py
```

## 2. Check what you are about to commit

```bash
git status --short
git diff --stat
```

Never stage any of these:

- anything in `game-data/`, `analysis/` or `save-game/`;
- `*.pkg`, `*.crs`, `*.rsc`, `*.str`, `*.sav`, `*.exe`, `*.dll`;
- `.venv/`, `target/`, `dist/`;
- PNG captures, emulator output, shader disassembly.

`.gitignore` covers most of these. Never use `git add -f` to get past it.

## 3. Stage only your own work

Other people, and other agents, may edit the repository at the same time. So:

- never use `git add -A`, `git add .` or `git commit -a`;
- stage whole files that only you changed: `git add path/one path/two`;
- in a shared file that also holds someone else's changes, stage only your own changes. `git add
  -p` does this interactively. Without an interactive terminal, build the version you want to
  stage and write it straight into the index:
  ```bash
  git show HEAD:apps/game/src/main.rs > "$TMP/main.rs"   # then apply only your edits to it
  blob=$(git hash-object -w "$TMP/main.rs")
  git update-index --cacheinfo 100644,$blob,apps/game/src/main.rs
  ```
- check the result with `git diff --cached --stat`;
- never revert changes that you did not make.

## 4. Run the checks

Run the checks in [Testing](../TESTING.md#before-every-commit) before you commit.

## 5. Commit and push

The subject line is in the imperative and has at most 72 characters, for example
`Port toolbar root level` or `Trace status bar counters`. The body says what was traced or
ported, and how it was verified. AI agents end the message with their attribution line, for
example `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

```bash
git commit -F message.txt
git show --stat HEAD                  # check that only your files went in
git pull --rebase --autostash && git push
```

If the rebase has a conflict in files that you did not touch, stop and ask. Do not resolve
someone else's work.

Commit and push after each piece of work that is complete and verified, not only at the end.
