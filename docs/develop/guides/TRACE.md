# 1. Trace a behaviour

Tracing means finding the original functions that do something, and understanding them well
enough to port them. The evidence is static: you read the decompiled pseudocode, the
disassembly and the program's data. You do not run the program.

Label every conclusion in one of three ways:

- **observed**: you read it in the code or the data;
- **inferred**: it follows from what you read, but you did not see it directly;
- **unknown**: you do not know yet. List it under "Open questions" in the document.

A trace of a code path does not prove that the path runs. To prove that, follow the callers up
to a loop or a virtual function table that the game uses. For numbers, use the
[emulator](EMULATE.md).

## Step 0: check what is already known

```bash
grep -rn "FUN_1400be770\|Toolbar" docs/ | head      # by address or name
python3 tools/triage/fnindex.py info 0x1400be770     # callers, callees, strings, traced or not
```

`fnindex.py` answers the routine questions quickly:

| Command | Question |
|---|---|
| `fnindex.py info ADDR` | What does this function call, who calls it, which strings and classes does it use, and is it traced? |
| `fnindex.py frontier ADDR --depth 2` | Which untraced functions does this feature still need, largest first? |
| `fnindex.py hot` | Which untraced functions are called most by traced code? |
| `fnindex.py find CitizenAIComponent` | Which functions use this string or class name? |
| `fnindex.py coverage` | How much of the program is traced? |

A function counts as traced when its address appears in `docs/`, `crates/`, `apps/` or `tools/`.
So when you name the original function in a comment or a document, it is marked as done.

## Step 1: read a function

Use `tools/triage/decomp.py` to read a function. It is easier to read than the raw export: it
names the calls, the global values (with float constants written out), the virtual function
tables and the known structure fields.

```bash
python3 tools/triage/decomp.py 0x1400be770
python3 tools/triage/decomp.py 0x1400be770 --asm        # with the disassembly
python3 tools/triage/decomp.py 0x1800ca560 --binary Runtime-steam-x64.dll
python3 tools/triage/types.py class CitizenAIComponent  # a class: size, factory, vtable slots
python3 tools/triage/types.py owner 0x1400be770         # which class this function belongs to
```

When you identify a function or a field, add a line to `tools/triage/names.tsv` (functions) or
`tools/triage/fields.tsv` (fields). Every tool then shows your name instead of `FUN_…` or
`+0x44`. This helps everyone who reads the code after you.

## Step 2: find an entry point

Start from something concrete that you can search for:

- **A string.** Resource paths (`"Dialog/Toolbar.rsc"`), interface element names
  (`"toolbarContent"`) and class names appear as text in the pseudocode.
  ```bash
  D=analysis/export/Application-steam-x64.exe
  grep -n 'Dialog/Toolbar' $D/decomp.c | head
  ```
- **A known caller.** Search the pseudocode for `FUN_1400be770` to find the functions that call
  it.
- **An engine class.** The Runtime exports its classes by C++ name. Its `decomp.c` uses those
  names, for example `UI::Element::SetChild`. Serialisers (`Serialize`), the interface, files and
  maths are in the Runtime.
- **A resource type.** A resource's type id is the CRC-32 of its class name in UTF-16LE.
  `tools/schema/types.json` maps the known ids to names. Search the pseudocode for the id as a hex
  constant to find the registration table, which leads to the class's factory.
- **A virtual function table.** A constructor stores the table's address in the object
  (`*param_1 = &PTR_…`). List its slots:
  ```bash
  python3 tools/ghidra/vtable.py game-data/original/Application-steam-x64.exe \
    $D/functions.tsv 0x140165120 40
  ```
- **A constant.** Ghidra shows many float literals as references to data (`DAT_1401…`). Read
  their values:
  ```bash
  python3 tools/ghidra/readdata.py game-data/original/Application-steam-x64.exe f32 0x14016a2b0
  ```
  The types are `f32`, `f64`, `i32`, `u32` and `hex16`.

## Step 3: decompile what Ghidra missed

Some functions are reached only through tables, and the bulk export does not include them.
Decompile them on demand in the existing Ghidra project:

```bash
python3 tools/triage/decomp.py 0x1400be770 --headless
```

Each headless Ghidra run needs about one minute to start, so decompile several addresses at once
when you can. Ghidra locks a project while it runs, so do not run two jobs on the same project
at the same time. `tools/ghidra/` has more scripts: `ExportNamedFunctions.java` (by name),
`ExportVtableSlots.java` and `ExportAddressRefs.java`. Their output goes to `analysis/`.

## Step 4: check the disassembly

Ghidra's pseudocode is not always right. It can drop loop bounds, give floats the wrong type,
merge two stack variables, and lose arguments that are passed in registers. Before you port
arithmetic, a loop or a bit operation, read the instructions:

```bash
objdump -d --no-show-raw-insn --start-address=0x1400be680 --stop-address=0x1400be770 \
  game-data/original/Application-steam-x64.exe > analysis/be680.s
```

The programs use the Windows x64 calling convention: the first four arguments are in `rcx`,
`rdx`, `r8` and `r9` (floats in `xmm0` to `xmm3`), the result is in `rax` or `xmm0`, and `this`
is in `rcx`. Look for these details:

- `cvttss2si` truncates a float to an integer. In Rust, that is `as i32`.
- The order of the operands of `minss` and `maxss` decides the result when one value is NaN.
- `movss` is single precision and `movsd` is double precision.

## Step 5: rebuild the structures

Keep a table of each class's fields as you go: the offset, the type and the meaning, for
example `+0x44 enabled (u8)`. A class's `Serialize` function gives the clearest order and types
of its fields. Read it first, then the functions that use the fields. Interface element ids,
event codes (for example `0x12` for a click) and enum values belong in the document's tables.

## Step 6: write it down

Write what you found straight into the topic document in `docs/`. [Document and land](LAND.md)
describes the style. Keep scratch output in `analysis/`. Never paste pseudocode or disassembly
into `docs/` or into the source code.

## Common mistakes

- Names inside packages use backslashes (`Game\Toolbar_x.crs`), but references use slashes and
  an object name (`Game/Toolbar.rsc:resource`). Normalise both before you compare them.
- Many small functions are the same allocator or string code. Check whether a `FUN_` is a thunk
  to the Runtime in `functions.tsv` (`RUNTIME-STEAM-X64.DLL::…`).
- For large or subtle numeric code, stop guessing and [emulate](EMULATE.md) it.
