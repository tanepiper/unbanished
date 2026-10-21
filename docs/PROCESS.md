<p align="center"><img src="assets/un-banished-logo.svg" alt="Un-Banished" width="560"></p>

# How Un-Banished was made

This page explains, in plain language, how Un-Banished was built. We did not have the source code
of Banished. We had only the finished game: the program files and the data files. From these we
worked out how the game behaves, and then we wrote a new engine that behaves the same way.

The work used three tools and one person:

- **Ghidra**, a free program that turns machine code back into readable code.
- **REA** (Reverse Engineer Anything), a toolkit and method for studying finished programs.
- **An AI coding agent** (Claude Code), which did most of the reading, testing and writing.
- **A human**, who chose the goals, played the game, found problems and made the decisions.

The sections below explain each part and how they worked together.

- [What we started with](#what-we-started-with)
- [The tools and who did what](#the-tools-and-who-did-what)
- [The process, step by step](#the-process-step-by-step)
- [An example: the name generator](#an-example-the-name-generator)
- [What "from scratch" means here](#what-from-scratch-means-here)
- [How the agent was guided](#how-the-agent-was-guided)
- [In numbers](#in-numbers)
- [Limits](#limits)

## What we started with

A copy of Banished 1.0.7 (Steam build 170910) contains two kinds of files.

**Program files.** These are the parts that run:

| File | What it does |
|---|---|
| `Application-steam-x64.exe` | The game itself: the rules, the citizens, the menus |
| `Runtime-steam-x64.dll` | The engine: files, memory, maths, sound, the interface system |
| `VideoDX9` / `VideoDX11` | Drawing on screen with Direct3D |

These files contain machine code. Machine code is a long list of numbers that the processor
runs. People cannot read it directly.

**Data files.** These are four `.pkg` packages in the `WinData` folder. They hold the art, the
sound, the text, the menus, and many numbers for the game rules (for example, how long a tree
takes to grow).

We did not have the source code. Source code is the readable text that the original developer
wrote and then turned into machine code. So the first problem was to understand the program
without it.

## The tools and who did what

### Ghidra: from machine code to readable code

[Ghidra](https://ghidra-sre.org/) is a free tool for studying programs. It reads machine code and
produces *pseudocode*: an approximate version of the code in a C-like language. Pseudocode has
no real names. A function is called `FUN_1400aa000`, which means "the function at address
`0x1400aa000`". A value is called `+0x44`, which means "the value 0x44 bytes into this object".

We ran Ghidra without its window, from a script (`tools/ghidra/export.sh`). The script analyses
both program files and writes every function's pseudocode into one large text file. This takes
about 20 minutes. The agent can then search that file like any other text.

The pseudocode stays on the local computer. It is in the `analysis/` folder, which git ignores,
so it is never published.

### REA: a method for studying finished programs

REA (Reverse Engineer Anything) is a toolkit that lets an AI agent inspect a finished program:
its files, its functions, its strings, and its behaviour while it runs. It also gives a strict
way of working:

- Study the program that was actually shipped.
- Connect every claim to evidence: an address, a value, or an observation.
- Keep a list of open questions instead of filling gaps with guesses.

This method shaped the whole project. Every finding in `docs/` says what kind of evidence it
rests on, and marks guesses as *inferred*.

REA can connect to Ghidra itself. In this project that connection was usually not set up, so the
agent read the separate Ghidra export described above. The documents say this openly. For
example, some of them note that a finding comes from the existing export and not from a new REA
session.

### The AI agent: reading, measuring, writing

The agent did most of the detailed work. For each part of the game it:

1. searched the pseudocode for the right function;
2. read it and explained it in plain words;
3. ran the original function to get exact results (see [Measure](#4-measure-run-the-original-code));
4. wrote new Rust code that does the same thing;
5. wrote a test that compares the new code with the original;
6. wrote down what it learned in `docs/`.

The agent worked inside rules written for it (see
[How the agent was guided](#how-the-agent-was-guided)). Several agent sessions often worked on
different parts of the game at the same time.

### The human: direction and judgement

The human did the things an agent cannot do alone:

- chose what to work on next, and in which order;
- played both games and reported differences (the [issues](https://github.com/tanepiper/unbanished/issues));
- ran the original game on a Windows PC and streamed it to a Mac, so the agent could see the real
  game and compare screenshots;
- decided what to do when the evidence was unclear, and what to publish.

### Summary

| Who or what | Main job | Output |
|---|---|---|
| Ghidra | Turn machine code into pseudocode | A local text file of all functions |
| REA | Give the method and the inspection tools | Evidence-first findings |
| AI agent | Read, measure, write code and tests | Rust code, tests, documents |
| Human | Choose goals, play, judge, decide | Priorities, bug reports, decisions |

## The process, step by step

Every part of the game went through the same loop. The diagram shows it; the text below explains
each step.

<p align="center"><img src="assets/process-loop.svg" alt="The seven steps of the work loop, from reading the data files to committing, with a path back to tracing when a check fails" width="1024"></p>

### 1. Read the data files

Before the game can run, it must read its packages. Each `.pkg` file contains hundreds of
resources: textures, models, sounds, menus, and rule tables. Each resource has a type, written
as a number.

The agent found the code in the program that registers each type. That code links each number to
a name, such as a texture or a dialog. With these names, the agent identified all 170 resource
types. Then it wrote readers for the formats the game uses, in `crates/formats`. A reader's test
reads every resource of its type in the real packages and checks that nothing is left over.

### 2. Decompile with Ghidra

Ghidra turned both program files into pseudocode, as described above. Each new agent session
starts from this export, so it does not need to run Ghidra again.

### 3. Trace: understand one function

Tracing means following the program from a known point to the part you need. For example, to
learn how a tree grows, start from the main game loop, find the call that updates trees, and read
that function.

The agent used several kinds of clues:

- **Strings.** Text such as `"Template/Citizen.rsc"` shows where the program uses a resource.
- **Exported names.** The engine file exports many functions with their real C++ names. These
  names show what many engine functions do.
- **Tables of functions.** C++ objects keep their functions in a table. Reading one table
  (`tools/ghidra/vtable.py`) shows all the actions an object type supports.
- **Constants.** Numbers stored in the program, such as a growth time, are read directly from
  the file (`tools/ghidra/readdata.py`).

The result of a trace is a short document in `docs/` (start at [FINDINGS.md](FINDINGS.md)),
written in prose and tables. It names the
functions by address, describes what they do, and lists the open questions. It never copies the
pseudocode.

### 4. Measure: run the original code

Reading code can be wrong. The order of a calculation can change the last digit of a decimal
number. In a simulation, small errors grow over time. So for any function with
numbers in it, the agent measured the original directly.

For this the project has its own emulator, `tools/emu/pe_emu.py`, built on the
[Unicorn](https://www.unicorn-engine.org/) library. An emulator is a program that pretends to be a
processor. The harness:

1. loads the original game and engine files into memory, at the addresses they expect;
2. connects the game to the engine, as Windows would;
3. replaces the operating-system parts with small stand-ins;
4. calls one original function with chosen inputs, and records what it returns.

The game itself never starts. Only the one function runs, thousands of times, with different
inputs. The results are saved in the local `game-data/derived/emu/` folder. There are emulator
scripts for random numbers, terrain, path finding, tree growth, names, storage, picking, animation
and more.

### 5. Rebuild in Rust

Now the agent writes new code in [Rust](https://www.rust-lang.org/). The new code follows the
notes from step 3, not the pseudocode line by line. It uses clear names, Rust types, and the
project's own structure.

Each ported function says where its behaviour comes from, in a comment such as
`/// Port of FUN_1400be680: …`. If something is a guess, the comment says *inferred*. If the new
code behaves differently on purpose, the comment says `Deviation:`, and a script collects all
deviations into [DEVIATIONS.md](DEVIATIONS.md).

Rendering is rebuilt in the same way. The original draws with Direct3D shaders. The agent read
those shaders and wrote new versions in WGSL for [wgpu](https://wgpu.rs/), which runs on Vulkan,
Metal, DirectX 12 and in the browser.

### 6. Check against the original

The project checks the new code in four ways:

- **Tests against the emulator.** A Rust test reads the saved results from step 4 and checks that
  the new code gives exactly the same answers. Exactly means bit for bit, including decimal
  numbers. These tests skip when the game data is missing, so they never need game files in the
  repository.
- **Determinism.** `tools/triage/determinism.sh` runs the same new game twice with the same seed
  and checks that the two pictures are identical. This catches hidden randomness.
- **Save files.** Saves from the original game must load. Their contents must read back
  correctly. - **Pictures of the real game.** The original runs on a Windows PC and streams to a
  Mac with Steam
  Remote Play. The agent takes screenshots of both games in the same situation and compares them
  with an image-difference tool.

When a check fails, the work goes back to step 3.

### 7. Write it down and commit

The agent updates the documents, ticks the item in
[GAME-LOOP-CHECKLIST.md](GAME-LOOP-CHECKLIST.md), and commits only its own files. The commit
message says what was traced and how it was checked.

## An example: the name generator

Every citizen in Banished has a name. Each new town gets a name too. The names look real but
are invented. The game builds each name one letter at a time. A database lists, for every group of
four letters, which letters may come next. The game picks one of them at random, moves on by one
letter, and repeats. Here is how that part was rebuilt.

1. **Read.** The packages contain three name databases: male names, female names and town names.
   The agent wrote a reader for their format.
2. **Trace.** The agent found two functions. `FUN_1400a9d50` builds one name. `FUN_1400aa000`
   calls it up to 50 times and keeps the first name that is long enough and is not a real name
   from the database. The agent worked out how each letter is chosen with the game's random
   number generator, and how the generator decides where a name ends.
3. **Measure.** The script `tools/emu/names.py` runs the original generator in the emulator. For
   each database and for random seeds 1 to 40, it makes 25 names in a row. That is 3,000 names
   from the original code.
4. **Rebuild.** The agent wrote the generator again in Rust, in `crates/game`.
5. **Check.** The test `crates/game/tests/names_emu.rs` makes the same 3,000 names with the new
   code and compares them with the original. All of them match. Because the names come one after
   another from one random sequence, this also proves that the new code uses random numbers in
   exactly the same order as the original.
6. **Record.** The findings are in [TOWN-NAME-DATABASE.md](TOWN-NAME-DATABASE.md).

The same pattern was used for terrain generation, path finding, tree growth, storage, animation
and many other systems.

## What "from scratch" means here

"From scratch" means:

- **All code in this repository is new.** It was written for Un-Banished, in Rust.
- **No original code is copied.** We read the pseudocode to understand the game. We did not
  translate it line by line, and we never put it in the repository.
- **No game content is included.** No art, sound, text, data files or program files. You need
  your own copy of the game, and Un-Banished reads it on your computer.
- **The documents describe, they do not copy.** They use prose, tables, addresses and numbers.

The project has strict rules to keep it this way. The `game-data/` and `analysis/` folders are
ignored by git. The rules in [AGENTS.md](../AGENTS.md) forbid committing game material or pasting
pseudocode into any tracked file. The only images of the game in the repository are a few
screenshots of Un-Banished running, used to show the project.

We do not call this a "clean-room" reimplementation. In a clean room, the people who study the
original are kept apart from the people who write the new code. Here, the same agent did both.

## How the agent was guided

An AI agent works best with clear rules, clear steps and fast checks. The first commits
(14 October 2026) were about reading the data files. Almost everything the agent uses today was
added after that, during the work, by the agent itself. The human approved each new rule.

The model itself did not change. What improved was its **harness**: the rules, guides, tools,
shared notes and checks around it. Each new session starts with everything the earlier sessions
added. This is what we mean when we say the harness improved itself.

### The parts of the harness

| Part | What it is | Where |
|---|---|---|
| Rules | The rule book every session reads first | [AGENTS.md](../AGENTS.md) |
| Skills | Step-by-step guides, one for each kind of task | `.claude/skills/` |
| Shared notes | Names for original functions and struct fields, used by every tool | `tools/triage/names.tsv`, `fields.tsv` |
| Triage tools | Programs that answer "what does this function do?" and "what should we do next?" | `tools/triage/` |
| Checks | Tests and comparisons that say yes or no without a human | `tools/emu/`, `tools/triage/`, `tools/compat/` |
| Findings | What was learned, written down with evidence | `docs/` |
| Memory | Short notes from the human's corrections, kept between sessions | Claude Code's memory (outside the repository) |

**The rules.** [AGENTS.md](../AGENTS.md) has six main rules:

1. Never commit game material.
2. Find evidence before writing code, and cite the original function.
3. Mark every intentional difference as a deviation.
4. Other sessions work at the same time, so commit only your own changes.
5. Commit and push after each finished piece of work.
6. Ask before downloading anything.

The file was changed in 18 commits. Most changes record a new tool or a new way of working,
so that the next session knows about it.

**The skills.** The agent loads one of these guides for each kind of task:

| Skill | Task |
|---|---|
| `unbanished` | The starting point: the ground rules, and which skill to use |
| `banished-trace` | Find and understand an original function, constant or table |
| `banished-emulate` | Run an original function in the emulator to get exact results |
| `banished-format` | Read a new kind of resource or save-file block |
| `banished-port` | Turn traced behaviour into Rust |
| `banished-shader` | Rewrite an original shader in WGSL |
| `banished-verify` | Run the tests, captures and browser build |
| `banished-land` | Write the documents, commit and push |
| `banished-video` | Record a short video of the game without a window |

**The checklist** ([GAME-LOOP-CHECKLIST.md](GAME-LOOP-CHECKLIST.md)) lists what is done and what
is next. Each new session reads it first, so work does not start from zero.

### How the harness grew

The pattern was always the same. A task was slow, or a mistake happened. The agent fixed the
problem, and then it changed the harness so that the problem would be easier, or impossible, the
next time. The table shows real examples from the project's history.

| Problem | Change to the harness | When |
|---|---|---|
| The same steps were repeated in every task: trace, measure, port, check, write down | The agent wrote the steps down as skills, one for each kind of task | 14 Oct |
| Raw Ghidra output is hard to read: calls, fields and constants have no names | `decomp.py` names calls, fields, vtables and float constants. `names.tsv` and `fields.tsv` collect every name an agent finds (now 230 functions and 146 fields) | 15 Oct |
| Hard to choose what to work on next | `fnindex.py hot` and `frontier` list the untraced functions that matter most | 15 Oct |
| "Is this function done?" had no clear answer | A function counts as traced when its address is named in the code or the docs. `coverage.py` and `fnindex.py coverage` count them | 15–17 Oct |
| Random numbers drawn in a different order make a town grow differently | `rng_trace.py` records the original's draws in the emulator. `rngdiff.py` compares them with the port's draws | 15 Oct |
| Intentional differences were scattered through the code | Every difference has a `Deviation:` comment. `deviations.py` collects them into [DEVIATIONS.md](DEVIATIONS.md) | 15 Oct |
| Hard to compare pictures with the original game | Reference frames are streamed from the original on Windows (Steam Remote Play). `imgdiff` measures how two screenshots differ | 15 Oct |
| Hard to check that two runs are the same | `determinism.sh` runs two new games with the same seed and compares the frames | 15 Oct |
| Rules were spread over the skills, and other agent tools could not read them | [AGENTS.md](../AGENTS.md) collects them in one file that every agent tool reads | 17 Oct |
| Several sessions edit the repository at the same time | Rule 4: stage only your own files. Sessions send each other short messages about which files they are editing | 17–19 Oct |
| During the speed-up work, a save from the new build differed from the old one | The cause was the test, not the change: the save was written a varying number of frames late. Headless captures now wait for the GPU, and `tools/compat/` checks any two versions against each other. The rule: find the exact cause, then fix it or revert the change | 19 Oct |
| Moving code between files by hand is slow and risky | `tools/refactor/` scripts move code without changing it. The compiler and the tests check the result. When a script bug cut an item short, the script was changed to write nothing until every item is found | 19 Oct |
| The API documentation was full of addresses and offsets | Rule 2 now says that doc comments are plain English and end with one "Original:" line. `doc_comments.py` helps with the rewrite. `deviations.py` learned to skip the new lines | 19 Oct |

The human's corrections were kept too. For example: "write English that translates easily" and
"an optimisation must not change any result". Claude Code keeps these as memory notes, so later
sessions follow them without being told again.

### Why it works

- **Every check is automatic and repeatable.** The tests, the emulator and the comparisons run
  without a window, with a fixed seed and a scratch save folder. Their answer is yes or no, so
  the agent can check its own work.
- **Evidence is written down.** Findings go into `docs/` and the shared name files, where
  every session can read them. Any session can continue the work.
- **Mechanical changes are checked by the machine.** When code is only moved, the compiler, the
  tests, the determinism check and the version comparison must all agree before a commit.
- **The human owns the rules.** A session may suggest a rule, but the human approves it. Sessions
  cannot change the rules for each other: when one session asked another to change
  `AGENTS.md`, the change waited for the human's answer.

### Many sessions at once

Several agent sessions often worked at the same time. For example, one session made the
code faster and cleaner while another wrote the website, the READMEs and the videos. They
worked like this:

- each session owns certain files, and stages only its own changes;
- a session sends a short message before it edits a shared file, and when it changes a path that
  the others depend on;
- one session checks another's work. When the documentation session rewrote the doc comments in
  all nine crates, the main session confirmed that only comments had changed, and that the game
  still produced identical saves and frames.

## In numbers

At the time of writing (19 October 2026):

| | |
|---|---|
| Lines of Rust | about 130,000 |
| Original functions traced (named in the code or the docs) | 2,050 of 9,846, which is 68 % of the game's code by size |
| Resource types identified | 170 of 170 |
| Emulator scripts that capture ground truth | 32 |
| Test files that compare with emulator results | 22 |
| Documents in `docs/` | about 120 |
| Commits | about 410, most of them written by the agent |
| Known deviations | 45, listed in [DEVIATIONS.md](DEVIATIONS.md) |

## Limits

The process is careful, but it is not perfect.

- **Not everything is measured.** Many functions are checked in the emulator, but not all.
  Some behaviour comes only from reading. Some of that reading may be wrong. - **Some
  differences are known.** [DEVIATIONS.md](DEVIATIONS.md) lists every place where the new
  code knowingly differs from the original, and why.
- **Some details are still open.** [PARITY-AUDIT.md](PARITY-AUDIT.md) shows what has been checked,
  and the [issues](https://github.com/tanepiper/unbanished/issues) show known problems.

If you find a difference from the original game, please
[report it](https://github.com/tanepiper/unbanished/issues/new/choose). Each report makes the
next version closer to the original.
