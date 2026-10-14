---
name: banished-format
description: Decode a Banished compiled resource (.crs/.rsc object), save-game block or package entry and add a dependency-free Rust parser to crates/formats with a whole-corpus test. Use when a resource type (Toolbar items, ToolbarDescription, dialogs, profiles, MenuScene, MapComponent, particle systems, sounds…) is not yet parsed or parses incorrectly.
---

# Decode a resource format into `crates/formats`

## Container facts (already solved — don't re-derive)

- `.pkg` = LZF-compressed entries; `pkg::Archive` merges `data1, patch1, patch0, data0` (that order).
- `.crs` container: blocks `0xaaaaaaaa | size | version | payload | 0xbbbbbbbb`. Outer payload:
  magic `0x5412a91f`, `typeId = CRC32(UTF-16LE class name)`, file version, dependency list, then
  **one nested block per class level** of `Serialize(Stream&)` (base class first).
- `String`: `u32 n|0x80000000` → n 8-bit bytes incl. NUL; else n UTF-16 units incl. NUL.
- External pointer (`ext`): `bool present` + `String reference`.
- Non-null `InternalBasePtr` in saves: flag byte + 12 bytes.
- References: `Path/File.rsc:object` (`:resource`, `:toolbar`, `:music`…); entry names in the
  archive use backslashes and `.crs` — normalise with `replace('\\', "/")`.

Full details: `docs/FINDINGS.md` §2–3, `tools/crs/crs.py`.

## Steps

1. **Find the serializer.** Get the type id from the file header (or `tools/schema/types.json`),
   then the class's `Serialize` in Runtime/Application decomp (`banished-trace`). Note each read in
   order with its type and the version gates (`if (version > 1)`).
2. **Prototype in Python** (fast iteration over the whole corpus):
   add a `@decoder('ClassName')` in `tools/crs/types/<area>.py` using the reader helpers
   (`r.u32() r.i32() r.f32() r.bool() r.string() ext(r) ext_list(r)`, `with r.block():` per class
   level), then
   ```bash
   python3 -m tools.crs.validate game-data/extracted ClassName
   ```
   Every file must decode **and consume every byte** of each block. Leftover bytes = missed field.
3. **Port to Rust** in `crates/formats/src/<area>.rs` (`game.rs` for gameplay resources, `ui.rs`
   for UI): a plain `pub struct` with documented fields (offset in the runtime object + meaning)
   and `pub fn name(data: &[u8]) -> Result<T>`. No external deps. Cite the serializer address in
   the doc comment.
4. **Corpus test** in `crates/formats/tests/<name>.rs` modelled on `toolbar.rs`: open the four
   packages from `../../game-data/original/WinData`, **return early with a message if missing**,
   select entries by normalised name, parse every one, assert a sane count (`names.len() > N`)
   and spot-check a known value.
   ```bash
   cargo test --release -p banished-formats --test <name>
   ```
5. **Optional inspector** `crates/formats/examples/<name>_info.rs` printing `{:#?}` for a reference
   (pattern: `dialog_info.rs`):
   `cargo run -p banished-formats --example dialog_info -- game-data/original/WinData "Dialog/Toolbar.rsc:resource"`.
6. Document the layout (field table, versions, which type ids) in a `docs/` topic file.

## Pitfalls

- Files of the same directory can be different classes (`Game\Toolbar_resource.crs` is an
  ExternalList, not a ToolbarItem) — filter by type id or exclude explicitly.
- Two adjacent empty strings look like a u32 0 — check against a sample with non-empty values.
- Bools are 1 byte; enums are u32/i32; colours are often 4×f32 or packed u32 — confirm in the
  serializer, not by eyeballing hex.
- Never commit sample resource bytes as test fixtures.
