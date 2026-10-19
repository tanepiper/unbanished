## What and why

<!-- What this changes, and the issue it closes (e.g. "Closes #12"). -->

## Evidence

<!-- For ported behaviour: the original functions it follows (FUN_1400be680, offsets, constants)
     and how they were checked (decompile, emulator, capture). Mark guesses as *inferred*.
     Never paste decompiled C, disassembly or resource dumps. -->

## Verification

<!-- Tests added or run, captures, the web build. Before/after screenshots for visible changes. -->

## Checklist

- [ ] Ported code cites the original function (`/// Port of FUN_…`); guesses say *inferred*.
- [ ] Intentional differences carry a `Deviation:` comment and `docs/DEVIATIONS.md` is regenerated
      (`python3 tools/triage/deviations.py`).
- [ ] `cargo test --offline -q --workspace --release --no-fail-fast` passes.
- [ ] `tools/triage/determinism.sh 22 1234` passes (simulation changes).
- [ ] `cargo build --offline --release -p banished-app --target wasm32-unknown-unknown` builds with 0 warnings.
- [ ] Changed files are `rustfmt --edition 2021` clean.
- [ ] Docs updated (`docs/`, `GAME-LOOP-CHECKLIST.md`, `USER-GUIDE.md` for player-facing changes).
- [ ] No game material: nothing from `game-data/`, `analysis/` or save files.
