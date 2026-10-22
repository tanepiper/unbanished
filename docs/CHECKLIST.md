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

## Labels and evidence

- [x] Function labels from the port's citations: `labels/names-from-port.tsv`, 477 labels
  (`tools/triage/cite_labels.py`)
- [ ] Move the 542 citations that are not on an item (module notes, notes inside bodies) onto the
  item that ports the function, so each one gives a label
- [ ] Recover the fields of the main classes (Citizen, Building, Storage) with GameFrame's
  `decomp_query.py layout`, and add them to `labels/fields.tsv` with evidence
