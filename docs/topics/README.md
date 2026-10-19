# How it works

These pages explain how parts of Banished work, and how Un-Banished rebuilds them. Each page is a
short introduction for readers who do not know the code. It links to the research notes in
`docs/`, which hold the full evidence: function addresses, field offsets, constants, and what is
still unknown.

| Topic | What it explains |
|---|---|
| [Game data files](GAME-DATA.md) | The `.pkg` packages, the compiled resources inside them, and type ids |
| [Exact maths and random numbers](EXACT-MATHS.md) | Why float rounding matters, the game's random numbers, game time |
| [Terrain generation](TERRAIN.md) | How a seed becomes a map: the five stages, rivers, cell types |
| [The game loop](GAME-LOOP.md) | Engine loops, the simulation step, game speed and the update buckets |
| [Citizens](CITIZENS.md) | How a citizen chooses what to do, and how it walks there |
| [Save files](SAVES.md) | The save container, the game state inside it, and loading |
| [The interface](INTERFACE.md) | Dialog resources, layout, controls and controllers |
| [Audio](AUDIO.md) | Sound resources, the WMA decoder, the device and sound effects |
| [The emulator harness](EMULATOR.md) | How original functions run in an emulator to give exact results |

## The research notes

The notes in `docs/` were written while the original was traced. They are evidence, not
tutorials. Good places to start:

- [FINDINGS.md](../FINDINGS.md): the program files, the package and resource formats, and the
  first decoded classes.
- [GAME-LOOP-CHECKLIST.md](../GAME-LOOP-CHECKLIST.md): what is traced and ported, and what is
  open, with links to every step.
- [PARITY-AUDIT.md](../PARITY-AUDIT.md): what has been checked against the original.
- [DEVIATIONS.md](../DEVIATIONS.md): every known, intentional difference from the original.
- [How it was made](../PROCESS.md): the method, from the program files to tested Rust code.

Each note starts with a paragraph that says what it covers and what kind of evidence it uses:
static reading of the decompiled code, the disassembly, emulation of original functions, or
decoded game data. A claim is either observed, marked *inferred*, or listed under
**Open questions**.
