# Checklist: Banished

Updated: 2026-10-22

The project's state in GameFrame's format: each line is an item, its main original function, and
its state (traced, measured, reimplemented, verified). The details of the game loop and the open
work are in [GAME-LOOP-CHECKLIST.md](GAME-LOOP-CHECKLIST.md). The parity of each system is in
[PARITY-AUDIT.md](PARITY-AUDIT.md).

## Inventory

- [x] Every binary identified: `Application-steam-x64.exe` (the game) and `Runtime-steam-x64.dll`
  (the engine, with about 4,900 exported C++ names), two renderer DLLs ([FINDINGS.md](FINDINGS.md))
- [x] Ghidra export of both binaries (`tools/ghidra/export.sh`, kept in `analysis/`)
- [x] Data containers identified: four LZF-compressed `.pkg` packages, 170 resource types
  ([GAME-TYPE-REGISTRATION-FINDINGS.md](GAME-TYPE-REGISTRATION-FINDINGS.md))

## Foundations

- [x] Random number generator: `System::Random`, Runtime `0x1800ca560` to `0x1800ca8b0`: verified
  (`crates/core/src/random.rs`)
- [x] Maths helpers: the CRT's `sinf`, `cosf`, `acosf`, `atan2f`, and the engine's transforms: verified
  (`crates/core/src/crt.rs`, `crates/core/src/math.rs`)
- [x] Game time and the update loop: integer microseconds, fixed steps: verified
  (`crates/core/src/scheduler.rs`)
- [x] Data container reader: `IO::PackageFile::ReadPackfile`, Runtime `0x1800b6f10`: verified on every
  package (`crates/formats/src/pkg.rs`)

## Systems

- [x] Terrain generation, rivers and the path finder: verified against the emulator
- [x] Saves: the original's saves load and continue: verified ([COMPATIBILITY.md](COMPATIBILITY.md))
- [ ] The open items of the game loop: production, citizen scheduling, weather effects and the
  rest ([GAME-LOOP-CHECKLIST.md](GAME-LOOP-CHECKLIST.md), "Remaining reverse-engineering work"
  and "Remaining port work")

## Coverage

A function is **traced** when it has a label: a hand label in `labels/names.tsv`, which names
the doc with its evidence, or a label from a ported item's citation in
`labels/names-from-port.tsv`. A citation is an ``Original: `FUN_…` `` line, or a `FUN_` address
in the comment directly above a function or a type. The coverage figure is the traced share of the binary's own code,
by size. Library code (`labels/library.tsv`, [LIBRARY-CODE.md](LIBRARY-CODE.md)), the
compiler's exception funclets and thunks are counted apart. `python3 tools/triage/fnindex.py
coverage` reports it, and no other tool gives a coverage figure. A function that is named in the
code or the docs without a label is reported as cited, and is not yet traced.

| 2026-10-27 | Own code | Traced | Cited, no label | Library code | Exception funclets |
|---|---|---|---|---|---|
| Application | 4,487 functions, 1,165,653 bytes | **1,496 functions, 59.7 %** | 580 functions, 16.4 % | 292 functions, 64,545 bytes | 5,056 functions, 73,373 bytes |
| Runtime | 6,858 functions, 968,747 bytes | **107 functions, 6.8 %** | 243 functions, 6.7 % | 682 functions, 144,503 bytes | 7,576 functions, 106,378 bytes |

The Runtime's figure is low because the port cites most engine functions by their exported
names (`System::Random`), which give no label.

## Labels and evidence

- [x] Class registrations and factory callbacks: 311 labels in `labels/names.tsv`
  (`Class::RegisterTypeInfo`, `Class::Create`, `Class::CreateInPlace`) from the 116 confirmed
  rows of [GAME-TYPE-REGISTRATIONS.tsv](GAME-TYPE-REGISTRATIONS.tsv), written by
  `tools/triage/type_labels.py`. 37 in-place callbacks are not functions in Ghidra's export and
  have no label
- [x] Function labels from the port's citations: `labels/names-from-port.tsv`, 1,109 labels
  (`tools/triage/cite_labels.py`). Citations on struct fields, enum variants, constants and
  module notes give labels too. A function's or a type's own name wins. A `FUN_` address in the
  comment directly above a function or a type gives a label when no ``Original:`` line names
  that address. Most of the 580 functions still cited without a label are named in notes
  inside function bodies, or only in the docs
- [x] Library code named by signature: 292 functions in the Application and 682 in the Runtime,
  with evidence for each (`labels/library.tsv`, `tools/triage/library.py`,
  [LIBRARY-CODE.md](LIBRARY-CODE.md)). Ghidra's Function ID matched 197 and 456 of them
- [x] One coverage figure from one tool (`fnindex.py coverage`, "Coverage" above)
- [x] Citations that gave no label: 531 of the 542 now give one (fields, variants, constants and
  module notes are read). 11 notes inside function bodies remain
- [x] Fields of the main classes: the citizen (CitizenComponent, CitizenAIComponent,
  CharacterComponent, CitizenDescription), the storage, the building components (WorkPlace,
  Residence, Build, Work, StateMachine, Destroy, ClearArea) and the Entity, Component and
  ControllerComponent bases. 227 rows in `labels/fields.tsv`, each with its type and the functions
  that read or write it ([CLASS-FIELDS.md](CLASS-FIELDS.md), which also lists where the port
  differs)
