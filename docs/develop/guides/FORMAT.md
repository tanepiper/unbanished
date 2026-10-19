# 3. Decode a resource format

The game's data is a set of serialised engine objects: dialogs, building templates, start
conditions, sounds, meshes and many more. A behaviour often needs one of them. If Un-Banished
cannot read it yet, you add a reader to `crates/formats`. This guide explains how.

[Game data files](../../topics/GAME-DATA.md) explains the containers. This guide assumes you know
that a resource is a `.crs` file inside a `.pkg` package.

## What is already solved

You do not need to work these out again:

- A `.pkg` package holds entries compressed with LZF. `pkg::Archive` merges the four packages
  in this order of priority: `data1`, `patch1`, `patch0`, `data0`.
- A `.crs` resource is a series of blocks. Each block is
  `0xaaaaaaaa | size | version | payload | 0xbbbbbbbb`. The outer payload starts with the magic
  number `0x5412a91f`, the type id, the file version and a list of dependencies. Then there is
  **one nested block for each level of the class hierarchy**, base class first, as the class's
  `Serialize(Stream&)` writes them.
- The type id is the CRC-32 of the class name in UTF-16LE.
- A string is a `u32` length, then the characters. If bit 31 of the length is set, the
  characters are 8-bit; if not, they are UTF-16. Both include the terminating zero.
- An external reference is a `bool` (present or not), then a string.
- A reference names an object inside a resource file, for example `Path/File.rsc:resource`.
  Inside the package, the same entry is named `Path\File.crs`.

The full details are in [FINDINGS.md](../../FINDINGS.md), sections 2 and 3, and in
`tools/crs/crs.py`.

## Steps

1. **Find the serialiser.** Read the type id from the file's header, or look it up in
   `tools/schema/types.json`. Then find the class's `Serialize` function in the Runtime or the
   Application ([Trace](TRACE.md)). Write down every read, in order, with its type and the
   version checks (`if (version > 1)`).
2. **Write a prototype in Python.** Python is faster to change, and the tools can run it over
   every file of the type. Add a decoder to `tools/crs/types/<area>.py`:
   ```python
   from ..crs import decoder
   from .ui import ext

   @decoder('ClassName')
   def class_name(r, header):
       # FUN_1400xxxxx: the serialiser this follows.
       with r.block():            # one block per class level
           return {'count': r.u32(), 'name': r.string(), 'target': ext(r)}
   ```
   The reader has `u32`, `i32`, `f32`, `bool`, `string` and `bytes`. The helpers `ext(r)` and
   `ext_list(r)` read external references. Then validate it over all files:
   ```bash
   python3 -m tools.crs.validate game-data/extracted ClassName
   ```
   Every file must decode, and **every byte of each block must be read**. Bytes left over mean a
   field you missed.
3. **Port it to Rust**, in `crates/formats/src/<area>.rs` (`game.rs` for gameplay resources,
   `ui.rs` for the interface). Write a plain `pub struct` whose fields have doc comments (the
   offset in the runtime object, and the meaning), and a function
   `pub fn name(data: &[u8]) -> Result<T>`. Use the `crs::Reader`. Add no dependencies. Name the
   serialiser's address in the doc comment.
4. **Write a corpus test**, `crates/formats/tests/<name>.rs`. `toolbar.rs` is a model. Open the
   four packages from `game-data/original/WinData`. If they are missing, print a message and
   return. Select the entries by their normalised name, parse every one, check the count
   (`names.len() > N`), and check one known value.
   ```bash
   cargo test --release -p banished-formats --test <name>
   ```
5. **Optional: an inspector.** A small example program that prints a parsed resource helps the
   next person. `crates/formats/examples/dialog_info.rs` is a model:
   ```bash
   cargo run -p banished-formats --example dialog_info -- game-data/original/WinData \
     "Dialog/Toolbar.rsc:resource"
   ```
6. **Document the layout** in a document in `docs/`: a table of the fields, the versions, and the
   type ids.

## Common mistakes

- Files in the same folder can be of different classes. For example,
  `Game\Toolbar_resource.crs` is an `ExternalList`, not a `ToolbarItem`. Filter by type id.
- Two empty strings next to each other look like a single `u32` zero. Check against a file
  where the values are not empty.
- A `bool` is one byte. An enum is a `u32` or an `i32`. A colour is often four `f32` values or a
  packed `u32`. Confirm the type in the serialiser. Do not guess it from the bytes.
- Never commit bytes from a resource as a test fixture. Tests read your own game data.
