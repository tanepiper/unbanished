# API documentation

The libraries of Un-Banished are documented in two ways:

- **A README for each crate.** It says what the crate is for, lists its main types and
  functions, and gives examples. Start there. [Apps and crates](CRATES.md) links to every README.
- **The Rust API documentation.** `cargo doc` generates it from the doc comments in the source
  code. It lists every public module, type and function. The website publishes it at
  [/api/](https://tanepiper.github.io/unbanished/api/index.html), and it is rebuilt after each push to `main`.

## Why both

The READMEs are short and written by hand. They explain the purpose of a crate, the few types
that most callers need, and how to use them, with examples. They do not try to list everything.

The API documentation is complete, because every public item has its doc comment next to it.
So it is generated, not summarised by hand: a hand-written copy of every public item would go
out of date after the next change. Each crate's first page in the API documentation is its
README (`#![doc = include_str!("../README.md")]`), so the README's examples also run as doc tests
and stay correct.

## The doc-comment style

Doc comments (`///` and `//!`) are what rustdoc shows, so they are written for readers who do not
know the original program:

- **Plain English** that says what the item is or does, in the game's terms.
- **One last line that names the original**, for example ``Original: `FUN_1400d4860`.`` or
  ``Original: `Math::Vector3::Length`, Runtime `0x180022280`.`` This keeps the evidence that each
  ported item comes from the original program (rule 2 in [AGENTS.md](../../AGENTS.md)).
- **Offsets, vtables and `DAT_` constants go in ordinary `//` comments** next to the item. They
  are needed when you trace and port, so they stay in the source, where `tools/triage/fnindex.py`
  and `coverage.py` find them. rustdoc does not show them.
- **`Deviation:` and `not ported` sentences keep their wording**, because
  [DEVIATIONS.md](../DEVIATIONS.md) is generated from them.

```rust
// CropFieldComponent +0x48; the toggle in the field's window.
/// Whether farming is on.
pub farming: bool,

/// Plants the field when farming is on, the weather is warm enough and it is spring.
///
/// Original: `FUN_14011fd30` (state 2 update).
pub fn update_planting(&mut self) { … }
```

`tools/triage/doc_comments.py` helps:

```bash
python3 tools/triage/doc_comments.py check           # rendered doc lines that still show evidence
python3 tools/triage/doc_comments.py fix PATH         # move the evidence that is safe to move
```

`fix` moves only what it can move without changing the meaning, and keeps everything it moves.
Read its result: a sentence that listed fields by offset needs a person to rewrite it.

## The crates on /api/

| Crate | API documentation |
|---|---|
| `banished-core` | [banished_core](https://tanepiper.github.io/unbanished/api/banished_core/index.html) |
| `banished-terrain` | [banished_terrain](https://tanepiper.github.io/unbanished/api/banished_terrain/index.html) |
| `banished-path` | [banished_path](https://tanepiper.github.io/unbanished/api/banished_path/index.html) |
| `banished-game` | [banished_game](https://tanepiper.github.io/unbanished/api/banished_game/index.html) |
| `banished-formats` | [banished_formats](https://tanepiper.github.io/unbanished/api/banished_formats/index.html) |
| `banished-ui` | [banished_ui](https://tanepiper.github.io/unbanished/api/banished_ui/index.html) |
| `banished-gfx` | [banished_gfx](https://tanepiper.github.io/unbanished/api/banished_gfx/index.html) |
| `banished-audio` | [banished_audio](https://tanepiper.github.io/unbanished/api/banished_audio/index.html) |
| `banished-wma` | [banished_wma](https://tanepiper.github.io/unbanished/api/banished_wma/index.html) |
| `banished-mods` | [banished_mods](https://tanepiper.github.io/unbanished/api/banished_mods/index.html) |
| `banished-nexus` | [banished_nexus](https://tanepiper.github.io/unbanished/api/banished_nexus/index.html) |

The apps (`banished`, `banished-viewer`, `banished-cli`) are programs, not libraries. Their
READMEs describe how to use them.

## Build it yourself

```bash
cargo doc --no-deps -p banished-formats --open      # one crate, opened in your browser
RUSTDOCFLAGS="-W missing-docs" cargo doc --no-deps -p banished-ui   # list items without docs
site/scripts/build-api.sh                           # all the libraries, into site/public/api/
```

`build-api.sh` uses its own target folder, `target/api`. It builds with rustdoc's dark theme in
the website's colours (`site/scripts/api-header.html`) and writes an index page. On Linux, it
needs the ALSA headers, as a normal build does (`libasound2-dev`).
