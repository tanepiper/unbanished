# The work loop

These guides explain how a behaviour of the original game becomes tested Rust code in
Un-Banished. Every ported part of the game went through the same steps. The guides describe each
step, the tools for it, and the mistakes that are easy to make.

<p align="center">
  <img src="../../assets/process-loop.svg" alt="The steps of the work loop, from reading the data files to committing, with a path back to tracing when a check fails" width="1024">
</p>

## The steps

| Step | Guide | Result |
|---|---|---|
| 1 | [Trace](TRACE.md) | You know which original functions do the work, what they read and write, and what their constants are. |
| 2 | [Emulate](EMULATE.md) | You have the exact output of the original functions for many inputs. (Only for numeric work.) |
| 3 | [Decode a format](FORMAT.md) | Un-Banished can read the resource or save block that the behaviour needs. (Only when it cannot yet.) |
| 4 | [Port](PORT.md) | The behaviour is in Rust, and each function names the original it came from. |
| 5 | [Port a shader](SHADER.md) | A material draws as in the original. (Only for graphics.) |
| 6 | [Verify](VERIFY.md) | Tests, captures and the browser build show that the change works. |
| 7 | [Document and land](LAND.md) | The findings are written down, and the change is committed and pushed. |

A typical change runs: trace, then (emulate), then port, then verify, then land. If a check fails,
go back to tracing. Do not guess to make a test pass.

## What you need

- **Your own copy of Banished 1.0.7** (Steam build 170910, Windows x64), copied into the project
  with `tools/extract-assets.sh`. See [Building](../BUILDING.md).
- **Ghidra 12.1** with JDK 21, to decompile the original program. On macOS:
  `brew install ghidra`. Then run `tools/ghidra/export.sh` once. It takes about 20 minutes. It
  writes the readable pseudocode of every function to `analysis/export/<program>/decomp.c`.
- **Python 3** with a virtual environment for the emulator:
  `python3 -m venv .venv && .venv/bin/pip install unicorn pefile`.

Git ignores `game-data/` and `analysis/`. Nothing in them may be committed.

## The two original programs

| File | What it contains | Base address |
|---|---|---|
| `Application-steam-x64.exe` | The game's logic: citizens, buildings, the game loop, saves | `0x140000000` |
| `Runtime-steam-x64.dll` | The engine: files, maths, random numbers, the interface, sound | `0x180000000` |

Functions are named by their address, as Ghidra names them: `FUN_1400be770` is the function at
address `0x1400be770` in the program. The Runtime exports its functions with C++ names, so many
engine functions have real names, for example `UI::Element::SetChild`.

## Before you start

Read [GAME-LOOP-CHECKLIST.md](../../GAME-LOOP-CHECKLIST.md). It lists what is done and what is
still open. Then search `docs/` for the addresses, resource names or class names you are about
to work on. Much has already been traced:

```bash
grep -rn "FUN_1400be770\|Toolbar" docs/ | head
```

## The rules that matter most here

- Never copy decompiled code, disassembly or resource bytes into the repository. Describe what
  you found in words, tables, addresses and constants.
- Every ported function names the original function in its doc comment. A guess says
  *inferred* or *assumed*.
- Every intentional difference from the original has a `Deviation:` comment.

These guides were first written as instructions for AI coding agents. They are in
`.claude/skills/*/SKILL.md`. The guides here say the same things for people.
