---
name: banished-trace
description: Static reverse engineering of the original Banished binaries with the local Ghidra exports, headless Ghidra one-off decompiles, objdump, constant/vtable readers and the type-id map. Use when you need to find or understand how the original game implements something (a function, field offset, constant, vtable slot, dialog handler, resource serializer) before porting it.
---

# Trace behaviour in the original binary

Static evidence only: never claim the game "does" something at runtime unless it was emulated
(`banished-emulate`) or observed. Label conclusions **observed** (read in code/data), **inferred**,
or **unknown**.

## 0. Check what is already known

```bash
grep -rn "FUN_1400be770\|Toolbar" docs/ | head      # address or name
ls analysis/                                        # earlier one-off dumps (local only)
```

## 0b. Read and triage with tools/triage

Prefer `python3 tools/triage/decomp.py ADDR` over grepping decomp.c: it names calls (labels,
class vtable roles, `get<Type>`), constants (`DAT_1401787d4 /* 0.05f */`), vtables and struct
fields from `tools/triage/fields.tsv`, and collapses CheckPtr/array-append boilerplate
(`--asm`, `--raw`, `--headless` when the bulk export lacks a function). `types.py class NAME`
gives factory, size, constructor and every vtable slot with its lifecycle meaning; `types.py
owner ADDR` says which class a function belongs to. Record new function labels in
`tools/triage/names.tsv` and fields in `tools/triage/fields.tsv`.

`tools/triage/fnindex.py` (see `tools/triage/README.md`) answers the routine questions fast:
`info ADDR` (callers, callees, types, strings, traced?), `frontier ADDR --depth 2` (untraced
functions a feature still needs, largest first), `hot` (untraced functions most called from
traced code), `find CitizenAIComponent`, `coverage`. Naming an address in docs or code marks it
traced, so cite the functions you port.

## 1. Find an entry point

Work outward from something concrete — a resource path, string, type name, or a known caller.

- **Strings → code.** Resource paths (`"Dialog/Toolbar.rsc"`), UI element names (`"toolbarContent"`),
  type names. Grep the pseudocode, which inlines string literals:
  ```bash
  D=analysis/export/Application-steam-x64.exe
  grep -n 'Dialog/Toolbar' $D/decomp.c | head
  grep -n 'toolbarContent' $D/strings.txt
  ```
- **Function by address**: each function starts with `// ==== <addr> <name>`:
  ```bash
  awk '/^\/\/ ==== 1400be770 /{p=1;print;next} /^\/\/ ==== /{p=0} p' $D/decomp.c
  ```
- **Callers**: `grep -n 'FUN_1400be770' $D/decomp.c` then map line numbers back to the enclosing
  `// ====` header (`awk` with the last seen header).
- **Engine API**: the exe imports Runtime classes by mangled name; Runtime exports are listed in
  `analysis/export/Runtime-steam-x64.dll/exports.txt` and named in its `decomp.c` (e.g.
  `UI::Element::SetChild`). Runtime is the place to look for `Serialize`, UI controls, IO, Math.
- **Resource type IDs**: `typeId = CRC32(UTF-16LE class name)`. `tools/schema/types.json` maps known
  IDs; `python3 tools/schema/typeids.py` regenerates it. A serializer for type `0x852ec05b` is found
  by grepping the hex constant in decomp.c (registration tables pair the id with the factory).
- **Vtables**: find the table address in a constructor (`*param_1 = &PTR_…` / `0x14016…`), then
  `python3 tools/ghidra/vtable.py game-data/original/Application-steam-x64.exe $D/functions.tsv 0x140165120 40`.
  Engine loop vtables: slot order is the GameLoop interface (load/startup/update/draw/input…); check
  `docs/NEW-GAME-FLOW.md` for slots already identified.
- **Constants** (`DAT_1401xxxxx`, float literals Ghidra shows as data refs):
  `python3 tools/ghidra/readdata.py game-data/original/Application-steam-x64.exe f32 0x14016a2b0 0x14016a2b4`
  (types: `f32 f64 i32 u32 hex16`).

## 2. Decompile something Ghidra missed or got wrong

Callbacks reached only through tables are often not functions in the bulk export. Decompile
on demand in the existing project (no re-analysis):

```bash
G=/opt/homebrew/opt/ghidra/libexec
JAVA_HOME=/opt/homebrew/opt/openjdk@21/libexec/openjdk.jdk/Contents/Home \
"$G/support/analyzeHeadless" analysis/gameplay-project banished-gameplay \
  -process Application-steam-x64.exe -noanalysis -scriptPath tools/ghidra \
  -postScript ExportAddressBatch.java analysis/<topic>.txt 0x1400be770 0x1400be680
```
Analysed projects present locally (find others with `find analysis -name '*.gpr'`):
`analysis/gameplay-project banished-gameplay` (exe), `analysis/runtime-identity-project
banished-runtime` (Runtime DLL), `analysis/save-project banished-save`. `tools/ghidra/export.sh`
creates `analysis/ghidra-project banished` (all four binaries) if you need a fresh one.
Ghidra holds a lock per project: don't run two headless jobs on the same project at once.
Other scripts: `ExportNamedFunctions.java OUT term…` (by name substring, good for Runtime),
`ExportVtableSlots.java OUT label 0xTABLE count …`, `ExportAddressRefs.java`. Output goes to
`analysis/` (ignored). One headless run takes ~1 min of startup — batch addresses.

## 3. Verify against disassembly when it matters

Ghidra pseudocode drops loop bounds, mis-types floats, merges stack slots and loses arguments to
calls through registers. Before porting arithmetic, loops or bit tricks, read the instructions:

```bash
objdump -d --no-show-raw-insn --start-address=0x1400be680 --stop-address=0x1400be770 \
  game-data/original/Application-steam-x64.exe > analysis/be680.s
```
Windows x64 ABI: args `rcx, rdx, r8, r9` (floats `xmm0–3`), return `rax`/`xmm0`, `this` in `rcx`.
Watch for `cvttss2si` (truncation), `minss/maxss` operand order (NaN behaviour), `movss` vs `movsd`.

## 4. Reconstruct structures

Track field offsets per class as you go (`+0x44 enabled (u8)`, `+0xc0 hotkey`). Serializers
(`Serialize(Stream&)`) give the cleanest field order and types — read them first, then the
consumers. Element/UI ids, event codes (e.g. `0x12` click, `0x16` close), and enum values belong in
the doc table.

## 5. Record

Write findings straight into the relevant doc (see `banished-land`): function address → role,
field tables, constants with addresses, and an explicit "Open questions" list. Keep throwaway dumps
in `analysis/`. Never paste pseudocode or disassembly into `docs/` or source.

## Pitfalls

- Archive names inside `.pkg` use backslashes (`Game\Toolbar_x.crs`); references use `/` and an
  `:object` suffix (`Game/Toolbar.rsc:resource`). Normalise before comparing.
- Several functions share boilerplate (allocator, string ctor); identify `FUN_` thunks to Runtime
  via `functions.tsv` (`RUNTIME-STEAM-X64.DLL::…`).
- A trace of a code path is not proof it runs; find the caller chain up to a loop/vtable.
- For large or subtle numeric logic, stop guessing and emulate (`banished-emulate`).
